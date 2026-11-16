//! `halftone`: a print-style dot screen, where tone becomes dot size. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Input, Meta};
use crate::ops::blur::{gaussian_filter, is_fourier};
use crate::ops::cast;
use crate::ops::color::{luminance, vivid};
use crate::ops::dither::{check_matrix, matrix_greys};
use crate::ops::edge::EdgeOrConstant;
use crate::ops::inks::{
    CMYK_SCREEN_ANGLES, CONCENTRIC_DIRECTION, CONCENTRIC_STEP, Geometry, Inks, MOST_INKS, RGB_SCREEN_ANGLES, RINGS_REACH, SPIRAL_REACH, SPOT_SCREEN_STEP,
    Spot, shift, wave_reach,
};
use crate::ops::length::{float, whole};
use crate::ops::par;
use crate::ops::realism::Realism;
use crate::params::{Choice, Color, Params, PictureName};
use crate::{Error, Image, Inputs, Picture, Place};

/// Where `halftone` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Stylised,
    opacity: 1.0,
    inputs: &[Input::Place],
};

/// What `halftone` refuses: shape 'matrix' with no `matrix`, or one more than 256 pixels wide or high.
///
/// # Errors
/// [`Error::Recipe`] naming `matrix`.
pub(crate) fn check(params: &Params, inputs: &Inputs<'_>) -> Result<(), Error> {
    if params.shape != Shape::Matrix {
        return Ok(());
    }
    check_matrix("halftone", "shape 'matrix'", inputs)
}

/// What `halftone` reads around a pixel: the grid of dots is anchored at the picture's corner, so a row is not the same
/// in another frame of reference.
pub(crate) fn footprint(params: &Params) -> Footprint {
    // A pixel shows the tone at the centre of its screen cell, at most `far` cells away, and the tone is the
    // blur of the picture there, which reaches `int(truncate x sigma x cell + 0.5)` pixels. `cell` is a length in working px, whole or not.
    // Half a pixel for rounding the centre to a pixel and half for the blur's `+ 0.5`; the margin adds the
    // interpolation rows.
    const HALF_DIAGONAL: f64 = std::f64::consts::FRAC_1_SQRT_2; // constant: a cell's corner is this many cells from its centre
    // `far`: a grid's cell is half a diagonal across; a wave's pixel and cell centre are a sine apart besides, at most twice the
    // amplitude; a ring's wedge and a spiral's cell, which are about a cell long and wide, measured (`ops::inks`'s test), are at most 0.74
    // and 1.04 away, bounded by `RINGS_REACH` and `SPIRAL_REACH`; rays widen with the distance
    // from the centre, which the footprint does not know, so a ray's pixel may read from anywhere: the whole picture.
    let far = match params.screen {
        Screen::Grid => HALF_DIAGONAL,
        Screen::Wave => wave_reach(params.amplitude),
        Screen::Rings => RINGS_REACH,
        Screen::Spiral => SPIRAL_REACH,
        Screen::Rays => return Footprint::Whole,
    };
    let cell = f64::from(params.cell);
    let blur = f64::from(params.truncate) * f64::from(params.sigma);
    Footprint::reach(
        (far + blur) * cell + 1.0,
        params.edge == EdgeOrConstant::Wrap && params.sigma > 0.0,
    )
    .transformed(params.sigma > 0.0 && is_fourier(cell * f64::from(params.sigma), f64::from(params.truncate)))
}

/// How far `halftone` paints beyond the picture's edge: as far as it reads.
pub(crate) fn spread(params: &Params) -> crate::Spread {
    crate::Spread::of_footprint(footprint(params))
}

/// A print-style dot screen: tone becomes dot size.
///
/// A rotated square grid of dots, `cell` px apart, turned `angle` degrees. Each dot looks up the picture, blurred by a
/// Gaussian of `sigma` cells, at its centre, and is as large as the ink that tone needs: the area of a dot is
/// proportional to the ink, so the screen keeps the picture's tones: dots touch at about 78.5 % tone and fill their cell at
/// full tone. Ink is the Rec. 709 luminance on a dark `paper` (below `paper_threshold`) and its complement on a light one. The
/// dot's edge is a ramp `feather` px wide, centred on its radius and faded in with the dot so that a dot narrower than
/// the ramp prints about its own area (and paper between dots narrower than it fades out, so near-full tones fill), and bare paper for zero ink. A dot is `ink`, or with no `ink` the colour of the
/// photo under it, scaled up until its strongest channel is full but by at most 1 / `ink_floor`.
///
/// `shape` 'line' draws parallel lines `cell` px apart instead of dots, along the screen's rows, whose width is the ink
/// (a line as wide as `cell` fills it), so the screen keeps the tones the same way; 'square', 'ellipse', 'diamond', 'euclidean'
/// and 'matrix' are other marks that fill their cell the same way, as large as the ink needs. `separation` prints several inks,
/// each its own screen, subtractively over `paper` ('rgb' adds light instead; inks overlap like transparent layers, so use a light
/// `paper`, as the default black paper prints black): 'cmyk' prints cyan, magenta, yellow and black, found from the photo by black
/// generation (black is how dark the colour is, the rest is split between the three), with the screens at 15, 75, 0
/// and 45 degrees added to `angle` (set `angle` to 0 for the classic angles); 'spot' prints the `inks` (one to four
/// colours), finding how much of each makes the photo's colour (least squares on the colour relative to the paper, each amount 0 to 1), with the screens
/// `angle` + 30 degrees apart (45, 75, 105, 135 by default); 'rgb' adds red, green and blue light on black, one screen
/// per channel (as large as the channel's value), at 15, 45 and 75 degrees added to `angle`, white where all three are
/// full, and ignores `paper`. `ink`, `ink_floor` and `paper_threshold` are used only with no `separation`; a separation keeps the tone on 'grid' and
/// 'wave' screens only (see `screen`). `offset` px
/// moves the second ink's screen that far from the first along `offset_angle`, the
/// third twice as far and the fourth three times, as a misregistered press does. `dot_gain`, `ink_noise` and `specks` (with
/// `speck_size` and `seed`) print as a press does: fatter dots, uneven dots and bare specks in the ink; all 0 prints the clean screen.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How the rows of the screen run: 'grid' (straight, as the angle sets), 'wave' (each row a sine of `wavelength` cells
    /// and `amplitude` cells high), 'rings' (round the centre `x`, `y`), 'spiral' (an Archimedean spiral about it, a cell
    /// more each turn) or 'rays' (straight out from it, as many as make a cell about as wide as `cell` half way to the
    /// nearest edge, wider beyond and narrower within). With shape 'line' these are wavy lines, rings, a spiral and a
    /// sunburst; `angle` turns them. Every ink keeps the tone on every screen; with a `separation`
    /// each ink's rings, spiral or rays have a centre of their own, 4 cells further along the diagonal than the last, so that
    /// the inks overlap independently.
    #[param(default = Grid)]
    pub screen: Screen,
    /// With screen 'rings', 'spiral' or 'rays': the centre's horizontal position as a fraction of the way across the
    /// picture: 0 = the first pixel column, 0.5 = the middle, 1 = the last. Ignored with 'grid' and 'wave'.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1, advanced)]
    pub x: f32,
    /// With screen 'rings', 'spiral' or 'rays': the centre's vertical position as a fraction of the way down the picture: 0 =
    /// the first pixel row, 0.5 = the middle, 1 = the last. Ignored with 'grid' and 'wave'.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1, advanced)]
    pub y: f32,
    /// With screen 'wave': how far each row swings to either side, in cells; 0 = a grid.
    #[param(default = 1.0, unit = ratio, min = 0, max = 10, advanced)]
    pub amplitude: f32,
    /// With screen 'wave': the distance along a row between two crests, in cells.
    #[param(default = 6.0, unit = ratio, min = 1, max = 100, advanced)]
    pub wavelength: f32,
    /// What a mark is: 'dot' (round), 'square', 'ellipse' (its axes in the ratio `aspect`), 'diamond' (a square turned 45
    /// degrees), 'euclidean' (round in highlights, a diamond at 50 %, a round hole in shadows; a smooth cosine
    /// variant of the Euclidean spot function, not PostScript's piecewise one), 'matrix' (the spot
    /// function of your picture `matrix`) or 'line' (a line per cell row). Each is as large as the ink needs, so every shape
    /// keeps the tone.
    #[param(default = Dot)]
    pub shape: Shape,
    /// With shape 'ellipse': the ratio of the ellipse's long axis (along the screen's rows) to its short one; 1 = a round
    /// dot, below 1 is not taken (turn the ellipse with `angle` for a long axis the other way). The area stays the tone's. Ignored with any other shape.
    #[param(default = 1.0, unit = ratio, min = 1, max = 8, advanced)]
    pub aspect: f32,
    /// With shape 'matrix': the picture of one cell whose grey values are the thresholds at each place of it, the path of
    /// an image file in a recipe file (relative to the file) or in `fx`, or, in Python, a Picture or an array; in Rust,
    /// the name of a picture the photo was given or the path of an image file. At most 256 x 256 pixels, a side of any
    /// length. Its places are ranked by brightness and the brightest get ink first, equal ones in reading order (as
    /// `dither`'s `matrix`), so a picture of n places at tone k / n inks k of them. Ignored with any other shape; none
    /// is refused with 'matrix'.
    #[param(advanced)]
    pub matrix: PictureName,
    /// How many inks: 'none' (one ink, `ink` or the photo's colour), 'cmyk' (cyan, magenta, yellow and black), 'spot'
    /// (the colours in `inks`) or 'rgb' (red, green and blue light). Cmyk and spot print over `paper` like transparent
    /// layers, each in its own screen; 'rgb' adds light on black and ignores `paper`. The inks keep the picture's tone with `screen`
    /// 'grid' and 'wave'; on 'rings', 'spiral' and 'rays' their rows coincide and a separation prints off-tone.
    #[param(default = None)]
    pub separation: Separation,
    /// Distance between neighbouring dots in px (the screen's pitch), not necessarily a whole number. Larger = fewer, bigger dots, a coarser print.
    #[param(default = 14.0, unit = px, min = 1, max = 200)]
    pub cell: f32,
    /// Rotation of the dot grid in degrees, clockwise on screen: rows of dots run that many degrees below the x axis (340
    /// tilts them 20 degrees above it). The grid is square, so 90 degrees on looks the same; 45 is the classic screen
    /// angle for one ink.
    #[param(default = 45.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// With no `separation`: the ink: one colour for every dot ('#rrggbb', '#rgb' or three numbers 0..1); leave it out to give each dot the
    /// colour of the photo under it, brightened so small dots stay visible (by at most 1 / `ink_floor`).
    #[param(advanced)]
    pub ink: Option<Color>,
    /// With no `separation` and no `ink`: the most a dot's colour is brightened, as a floor 0.01 to 1 on its strongest
    /// channel: a colour whose strongest channel is below it is scaled by 1 / `ink_floor` instead of up to full, so dark
    /// areas stay dark. Lowering it brightens dark colours more; 1 = colours as they are.
    #[param(default = 0.35, unit = fraction, min = 0.01, max = 1, advanced)]
    pub ink_floor: f32,
    /// The paper colour between the dots; ignored with 'rgb' separation (always black). On a dark paper (Rec. 709 luminance below `paper_threshold`) bright tones get the
    /// ink; on a light one, dark tones.
    #[param(default = "#000000")]
    pub paper: Color,
    /// With no `separation`: the Rec. 709 luminance, 0 to 1, below which `paper` counts as dark (bright tones get the ink) rather than light
    /// (dark tones get it); 0.5 = mid grey. Raising it lets lighter paper count as dark: 0 treats every paper as light, 1
    /// every paper but white as dark.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1, advanced)]
    pub paper_threshold: f32,
    /// With 'spot' separation: the one to four ink colours ('#rrggbb', '#rgb' or three numbers 0..1), in the order they
    /// are offset and turned; ignored otherwise. Each pixel's colour is split into amounts of these inks.
    #[param(default = ["#ff4d6d", "#2b59ff"], min_length = 1, max_length = 4, advanced)]
    pub inks: Vec<Color>,
    /// With 'cmyk', 'spot' or 'rgb' separation: how far in px the second ink's screen is moved from the first (the third twice
    /// as far, the fourth three times), as ink misregistered on a press; 0 = all screens in register.
    #[param(default = 0.0, unit = px, min = 0, max = 100, advanced)]
    pub offset: f32,
    /// With an `offset`: the direction of the move, in degrees clockwise on screen from the x axis (90 is downward).
    #[param(default = 0.0, unit = degrees, min = 0, max = 360, cyclic, advanced)]
    pub offset_angle: f32,
    /// Gaussian sigma, in cells, of the blur that averages the tone each dot stands for: 0 = each dot sees only the pixel at
    /// its centre (detail, but noisy), larger = smoother tone that spreads into neighbouring dots.
    #[param(default = 0.5, unit = ratio, min = 0, max = 5)]
    pub sigma: f32,
    /// Width of each dot's soft edge in px (anti-aliasing): small = crisp dots, larger = blurrier dots. The ink stays the
    /// same.
    #[param(default = 1.0, unit = px, min = 0.1, max = 50)]
    pub feather: f32,
    /// Where the tone blur's Gaussian is cut off, in sigmas: it reaches truncate x sigma cells. Larger is more exact and
    /// slower.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// How much the press fattens dots, as presses quote it: the gain at 50 % tone, in tone (0.2 = a 50 % tint prints as 70 %).
    /// A tone `a` prints as `a + 4 x dot_gain x a x (1 - a)`: the simplest curve through 0 and 1 that is symmetric and
    /// gains `dot_gain` at 50 % (ISO 12647-2 quotes dot gain at 50 %; real presses gain 10 to 25 %), so highlights and
    /// shadows gain less. 0 = none.
    #[param(default = 0.0, unit = fraction, min = 0, max = 0.25, advanced)]
    pub dot_gain: f32,
    /// How uneven the ink is from dot to dot: each dot's coverage strays by a random share, up to this one, of the nearer of
    /// its ink and its paper (so a pure paper or full ink cell does not stray, and the mean ink stays the same). 0 = even.
    /// It varies the coverage of each dot, not each pixel: a press leaves one dot fat and the next thin, not grain.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1, advanced)]
    pub ink_noise: f32,
    /// Specks of bare paper in the ink: marks per 1000 design px², so any picture size looks alike; they show only where
    /// there is ink. 0 = none.
    #[param(default = 0.0, unit = density, min = 0, max = 5, advanced)]
    pub specks: f32,
    /// The diameter in px of a paper speck.
    #[param(default = 3.0, unit = px, min = 1, max = 10, advanced)]
    pub speck_size: f32,
    /// Random seed of `ink_noise` and `specks`: the same seed always gives the same picture, another seed another.
    #[param(default = 0, unit = count, min = 0, max = 4294967295, advanced)]
    pub seed: u32,
    /// What the tone blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored
    /// about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge), 'constant'
    /// (`constant_value`).
    #[param(default = Reflect, advanced)]
    pub edge: EdgeOrConstant,
    /// The brightness (0..1) assumed beyond the edge when edge is 'constant'; ignored otherwise.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1, advanced)]
    pub constant_value: f32,
}

/// How the rows of the screen run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Screen {
    /// Straight rows, `angle` below the x axis.
    Grid,
    /// Rows swung sideways by a sine of `amplitude` and `wavelength` cells.
    Wave,
    /// Rows round the centre `x`, `y`.
    Rings,
    /// One row, an Archimedean spiral about the centre, a cell more each turn.
    Spiral,
    /// Rows straight out from the centre.
    Rays,
}

/// What a mark of the screen is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Shape {
    /// A round dot in each cell.
    Dot,
    /// A line in each row of cells, wider where more ink is needed.
    Line,
    /// A square in each cell, whose sides run along the screen's rows.
    Square,
    /// An ellipse in each cell, its long axis along the screen's rows, in the ratio `aspect` to its short one.
    Ellipse,
    /// A diamond in each cell: a square turned 45 degrees, touching its neighbours at 50 % tone.
    Diamond,
    /// A round dot in highlights, a diamond at 50 % and a round hole in shadows, as the cosine spot function of print screens.
    Euclidean,
    /// The spot function of your picture `matrix`.
    Matrix,
}

/// How many inks the screen prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Separation {
    /// One ink: `ink`, or the photo's colour.
    None,
    /// Cyan, magenta, yellow and black, each at its own angle.
    Cmyk,
    /// The `inks`, each at its own angle.
    Spot,
    /// Red, green and blue light added on black, each at its own angle.
    Rgb,
}

/// One ink's screen: the grid turned by an angle and moved from the origin.
struct Turned {
    sin: f32,
    cos: f32,
    shift: [f32; 2],
    geometry: Geometry,
}

/// The dot a pixel's screen cell stands for, remembered until the pixel leaves the cell.
struct Cell {
    key: [u32; 2],
    sampled: [f32; 3],
    amounts: [f32; MOST_INKS],
    /// The spot level of each amount, solved once per cell, not per pixel.
    levels: [f32; MOST_INKS],
}

/// `value` rounded to a pixel index no further than `last`.
fn pixel(value: f32, last: usize) -> usize {
    whole(f64::from(value)).min(last)
}

/// The screens of the inks: each turned by `angle` and the ink's own extra angle, and moved by its share of the offset. The
/// centre of the rings, spiral and rays is a fraction of the canvas, not of the band of it a picture may be.
fn screens(params: &Params, place: Place, count: usize) -> Vec<Turned> {
    (0..count)
        .map(|ink| {
            let extra = match params.separation {
                Separation::None => 0.0,
                Separation::Cmyk => CMYK_SCREEN_ANGLES[ink],
                Separation::Rgb => RGB_SCREEN_ANGLES[ink],
                Separation::Spot => SPOT_SCREEN_STEP * float(ink),
            };
            let turn = (f64::from(params.angle) + extra).rem_euclid(360.0).to_radians(); // constant: a full turn in degrees
            let (sin, cos) = (cast::single(turn.sin()), cast::single(turn.cos()));
            let shift = shift(params.offset, params.offset_angle, ink);
            // the centre as a pixel finds its place in the turned grid, in cells
            let cell = params.cell;
            // each ink of a concentric screen has its own centre, or the inks' rows coincide
            let apart = match params.screen {
                Screen::Rings | Screen::Spiral | Screen::Rays => {
                    self::shift(CONCENTRIC_STEP * cell, CONCENTRIC_DIRECTION, ink)
                }
                Screen::Grid | Screen::Wave => [0.0; 2],
            };
            let (px, py) = (
                cast::single(f64::from(params.x) * float(place.canvas_width - 1)) - shift[0] + apart[0],
                cast::single(f64::from(params.y) * float(place.canvas_height - 1)) - shift[1] + apart[1],
            );
            let centre = [(px * cos + py * sin) / cell, (py * cos - px * sin) / cell];
            let geometry = match params.screen {
                Screen::Grid => Geometry::Grid,
                Screen::Wave => Geometry::Wave {
                    amplitude: params.amplitude,
                    wavelength: params.wavelength,
                },
                Screen::Rings => Geometry::Rings { centre },
                Screen::Spiral => Geometry::Spiral { centre },
                // a cell wide half way to the nearest edge of the canvas
                Screen::Rays => Geometry::rays(
                    centre,
                    cast::single(float(place.canvas_width.min(place.canvas_height)) / 2.0) / cell,
                ),
            };
            Turned {
                sin,
                cos,
                shift,
                geometry,
            }
        })
        .collect()
}

/// How much of each ink a dot standing for the colour `sampled` needs.
fn amounts(params: &Params, inks: &Inks, ink_is_bright: bool, sampled: [f32; 3]) -> [f32; MOST_INKS] {
    match params.separation {
        Separation::None => {
            let tone = luminance(sampled);
            [
                (if ink_is_bright { tone } else { 1.0 - tone }).clamp(0.0, 1.0),
                0.0,
                0.0,
                0.0,
            ]
        }
        Separation::Cmyk => inks.cmyk_amounts(sampled),
        Separation::Spot => inks.coverages(sampled),
        Separation::Rgb => Inks::rgb_amounts(sampled),
    }
}

/// The spot function of the mark `shape` names.
fn spot_of(params: &Params, inputs: &Inputs<'_>) -> Spot {
    match params.shape {
        Shape::Dot => Spot::Round,
        Shape::Line => Spot::Line,
        Shape::Square => Spot::Square,
        Shape::Ellipse => Spot::ellipse(params.aspect),
        Shape::Diamond => Spot::Diamond,
        Shape::Euclidean => Spot::euclidean(),
        Shape::Matrix => {
            let (greys, size) = matrix_greys(inputs);
            Spot::matrix(&greys, size)
        }
    }
}

/// The press `params` ask for.
fn realism(params: &Params) -> Realism {
    Realism {
        dot_gain: params.dot_gain,
        ink_noise: params.ink_noise,
        specks: params.specks,
        speck_size: params.speck_size,
        seed: params.seed,
    }
}

/// What shows where a speck bares the paper: the print of no ink.
fn bare_paper(params: &Params, inks: &Inks, paper: [f32; 3]) -> [f32; 3] {
    if params.separation == Separation::None {
        paper
    } else {
        inks.print(&[0.0; MOST_INKS])
    }
}

/// The amounts the press prints in the cell `cell`, where `wanted` is asked.
fn printed_amounts(real: &Realism, mut wanted: [f32; MOST_INKS], cell: [u64; 2]) -> [f32; MOST_INKS] {
    for (ink, amount) in wanted.iter_mut().enumerate() {
        *amount = real.coverage(*amount, ink, cell);
    }
    wanted
}

/// How much a speck covers the pixel (`x`, `y`) of the canvas, where some ink `coverages` is: a speck bares only ink.
fn speck_over(real: &Realism, coverages: &[f32; MOST_INKS], x: usize, y: usize) -> f32 {
    if coverages.iter().any(|&coverage| coverage > 0.0) {
        real.speck(x, y)
    } else {
        0.0
    }
}

/// The dot screen.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    // the screen is laid on the canvas, of which the picture holds the rows from `first_row`
    let place = inputs.place(picture);
    let canvas_last = place.canvas_height - 1;
    let cell = cast::single(f64::from(params.cell));
    let blurred;
    let smooth = if params.sigma > 0.0 {
        blurred = gaussian_filter(
            picture,
            f64::from(cell) * f64::from(params.sigma),
            f64::from(params.truncate),
            params.edge.extension(params.constant_value),
        );
        &blurred
    } else {
        picture.pixels()
    };
    let paper = params.paper.0;
    let inks = match params.separation {
        Separation::None => Inks::new(paper, &[]),
        Separation::Cmyk => Inks::cmyk(paper),
        Separation::Spot => Inks::new(paper, &params.inks.iter().map(|ink| ink.0).collect::<Vec<_>>()),
        Separation::Rgb => Inks::rgb(),
    };
    let count = match params.separation {
        Separation::None => 1,
        Separation::Cmyk => MOST_INKS,
        Separation::Spot => params.inks.len().min(MOST_INKS),
        Separation::Rgb => 3, // constant: red, green and blue
    };
    let screens = screens(params, place, count);
    let ink_is_bright = luminance(paper) < params.paper_threshold;
    let spot = spot_of(params, inputs);
    let real = realism(params);
    let bare = bare_paper(params, &inks, paper);
    let (inv_cell, inv_feather) = (1.0 / cell, 1.0 / params.feather);
    let mut pixels = vec![[0.0; 3]; height * width];
    par::rows(&mut pixels, width, |first, band| {
        let mut cells: Vec<Cell> = screens
            .iter()
            .map(|_| Cell {
                key: [u32::MAX; 2], // a NaN pattern, which no cell has
                sampled: [0.0; 3],
                amounts: [0.0; MOST_INKS],
                levels: [0.0; MOST_INKS],
            })
            .collect();
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let y = cast::single(float(place.first_row + first + offset));
            for (column, out) in line.iter_mut().enumerate() {
                let x = cast::single(float(column));
                let mut coverages = [0.0; MOST_INKS];
                for (index, (screen, remembered)) in screens.iter().zip(&mut cells).enumerate() {
                    let (sin, cos) = (screen.sin, screen.cos);
                    let (px, py) = (x - screen.shift[0], y - screen.shift[1]);
                    // the pixel in the rotated grid, in cells, and the centre of the cell it belongs to
                    let (u, v) = screen
                        .geometry
                        .forward((px * cos + py * sin) * inv_cell, (py * cos - px * sin) * inv_cell);
                    let (cu, cv) = (u.floor() + 0.5, v.floor() + 0.5);
                    if remembered.key != [cu.to_bits(), cv.to_bits()] {
                        // the tone at the centre, mapped back to the picture
                        let (gu, gv) = screen.geometry.back(cu, cv);
                        let column = pixel((gu * cos - gv * sin) * cell + screen.shift[0], width - 1);
                        let row = pixel((gu * sin + gv * cos) * cell + screen.shift[1], canvas_last);
                        // (a picture given fewer rows than the footprint says reads the nearest it has)
                        let row = row.clamp(place.first_row, place.first_row + height - 1) - place.first_row;
                        let sampled = smooth[row * width + column];
                        remembered.key = [cu.to_bits(), cv.to_bits()];
                        remembered.sampled = sampled;
                        let cell = [u64::from(cu.to_bits()), u64::from(cv.to_bits())];
                        remembered.amounts =
                            printed_amounts(&real, amounts(params, &inks, ink_is_bright, sampled), cell);
                        remembered.levels = remembered.amounts.map(|amount| spot.level(amount));
                    }
                    let which = if params.separation == Separation::None {
                        0
                    } else {
                        index
                    };
                    // the spot function's value here and its level for the ink: inked where the value is below the level
                    let (distance, radius) = (spot.value(u - cu, v - cv), remembered.levels[which]);
                    let fade_in = (radius * cell * 2.0 * inv_feather).min(1.0); // constant: the ramp is half a feather on each side
                    let ramp = ((radius - distance) * cell * inv_feather + 0.5).clamp(0.0, 1.0) * fade_in; // constant: the ramp is centred on the edge
                    // as a dot narrower than the ramp fades in, the paper between marks narrower than it fades out, so
                    // the ink reaches the full tone without a jump (the ramp alone leaves the cell's edge half paper)
                    let fade_out = ((1.0 - remembered.amounts[which].min(1.0)) * cell * 4.0 * inv_feather).min(1.0); // constant: the paper between marks fades over two feathers each side
                    coverages[index] = 1.0 - (1.0 - ramp) * fade_out;
                }
                let printed = if params.separation != Separation::None {
                    inks.print(&coverages)
                } else if coverages[0] > 0.0 {
                    let sampled = cells[0].sampled;
                    let dot = params.ink.map_or_else(|| vivid(sampled, params.ink_floor), |ink| ink.0);
                    [0, 1, 2].map(|channel| paper[channel] * (1.0 - coverages[0]) + dot[channel] * coverages[0])
                } else {
                    paper
                };
                let speck = speck_over(&real, &coverages, column, place.first_row + first + offset);
                *out = Realism::specked(printed, bare, speck);
            }
        }
    });
    Image::from_pixels(height, width, pixels)
}
