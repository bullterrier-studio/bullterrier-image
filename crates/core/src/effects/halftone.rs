//! `halftone`: a print-style dot screen, where tone becomes dot size. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::blur::gaussian_filter;
use crate::ops::cast;
use crate::ops::color::{luminance, vivid};
use crate::ops::edge::EdgeOrConstant;
use crate::ops::inks::{CMYK_SCREEN_ANGLES, Inks, MOST_INKS, SPOT_SCREEN_STEP, shift};
use crate::ops::length::{float, whole};
use crate::ops::par;
use crate::params::{Choice, Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `halftone` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Stylised,
    opacity: 1.0,
    inputs: &[],
};

/// What `halftone` reads around a pixel: the grid of dots is anchored at the picture's corner, so a row is not the same
/// in another frame of reference.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// A print-style dot screen: tone becomes dot size.
///
/// A rotated square grid of dots, `cell` px apart, turned `angle` degrees. Each dot looks up the picture, blurred by a
/// Gaussian of `sigma` cells, at its centre, and is as large as the ink that tone needs: the area of a dot is
/// proportional to the ink, so the screen keeps the picture's tones, dots touch at half tone and fill their cell at full
/// tone. Ink is the Rec. 709 luminance on a dark `paper` (below `paper_threshold`) and its complement on a light one. The
/// dot's edge is a ramp `feather` px wide, centred on its radius and faded in with the dot so that a dot narrower than
/// the ramp prints about its own area, and bare paper for zero ink. A dot is `ink`, or with no `ink` the colour of the
/// photo under it, scaled up until its strongest channel is full but by at most 1 / `ink_floor`.
///
/// `shape` 'line' draws parallel lines `cell` px apart instead of dots, along the screen's rows, whose width is the ink
/// (a line as wide as `cell` fills it), so the screen keeps the tones the same way. `separation` prints several inks,
/// each its own screen, subtractively over `paper` (inks overlap like transparent layers, so use a light `paper`; the
/// default black paper prints black): 'cmyk' prints cyan, magenta, yellow and black, found from the photo by black
/// generation (black is how dark the colour is, the rest is split between the three), with the screens at 15, 75, 0
/// and 45 degrees added to `angle` (set `angle` to 0 for the classic angles); 'spot' prints the `inks` (one to four
/// colours), finding how much of each makes the photo's colour (the nearest in optical density), with the screens
/// `angle` + 30 degrees apart (45, 75, 105, 135 by default). `ink`, `ink_floor` and `paper_threshold` are used only
/// with no `separation`. `offset` px moves the second ink's screen that far from the first along `offset_angle`, the
/// third twice as far and the fourth three times, as a misregistered press does.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// What a dot is: 'dot' (a round dot per cell), 'line' (a line per cell row, as wide as the ink needs).
    #[param(default = Dot)]
    pub shape: Shape,
    /// How many inks: 'none' (one ink, `ink` or the photo's colour), 'cmyk' (cyan, magenta, yellow and black) or 'spot'
    /// (the colours in `inks`). Several inks print over `paper` like transparent layers, each in its own screen.
    #[param(default = None)]
    pub separation: Separation,
    /// Distance between neighbouring dots in px (the screen's pitch). Larger = fewer, bigger dots, a coarser print.
    #[param(default = 14, unit = px, min = 1, max = 200)]
    pub cell: i32,
    /// Rotation of the dot grid in degrees, clockwise on screen: rows of dots run that many degrees below the x axis (340
    /// tilts them 20 degrees above it). The grid is square, so 90 degrees on looks the same; 45 is the classic screen
    /// angle for one ink.
    #[param(default = 45.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// The ink: one colour for every dot ('#rrggbb', '#rgb' or three numbers 0..1); leave it out to give each dot the
    /// colour of the photo under it, brightened so small dots stay visible (by at most 1 / `ink_floor`).
    #[param()]
    pub ink: Option<Color>,
    /// With no `ink`: the most a dot's colour is brightened, as a floor 0.01 to 1 on its strongest channel: a colour whose
    /// strongest channel is below it is scaled by 1 / `ink_floor` instead of up to full, so dark areas stay dark. Lowering it
    /// brightens dark colours more; 1 = colours as they are.
    #[param(default = 0.35, unit = fraction, min = 0.01, max = 1)]
    pub ink_floor: f32,
    /// The paper colour between the dots. On a dark paper (Rec. 709 luminance below `paper_threshold`) bright tones get the
    /// ink; on a light one, dark tones.
    #[param(default = "#000000")]
    pub paper: Color,
    /// The Rec. 709 luminance, 0 to 1, below which `paper` counts as dark (bright tones get the ink) rather than light
    /// (dark tones get it); 0.5 = mid grey. Raising it lets lighter paper count as dark: 0 treats every paper as light, 1
    /// every paper but white as dark.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub paper_threshold: f32,
    /// With 'spot' separation: the one to four ink colours ('#rrggbb', '#rgb' or three numbers 0..1), in the order they
    /// are offset and turned; ignored otherwise. Each pixel's colour is split into amounts of these inks.
    #[param(default = ["#ff4d6d", "#2b59ff"], min_length = 1, max_length = 4)]
    pub inks: Vec<Color>,
    /// With 'cmyk' or 'spot' separation: how far in px the second ink's screen is moved from the first (the third twice
    /// as far, the fourth three times), as ink misregistered on a press; 0 = all screens in register.
    #[param(default = 0.0, unit = px, min = 0, max = 100)]
    pub offset: f32,
    /// With an `offset`: the direction of the move, in degrees clockwise on screen from the x axis (90 is downward).
    #[param(default = 0.0, unit = degrees, min = 0, max = 360, cyclic)]
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
    /// What the tone blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored
    /// about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge), 'constant'
    /// (`constant_value`).
    #[param(default = Reflect, advanced)]
    pub edge: EdgeOrConstant,
    /// The brightness (0..1) assumed beyond the edge when edge is 'constant'; ignored otherwise.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1, advanced)]
    pub constant_value: f32,
}

/// What a mark of the screen is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Shape {
    /// A round dot in each cell.
    Dot,
    /// A line in each row of cells, wider where more ink is needed.
    Line,
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
}

/// One ink's screen: the grid turned by an angle and moved from the origin.
struct Screen {
    sin: f32,
    cos: f32,
    shift: [f32; 2],
}

/// The dot a pixel's screen cell stands for, remembered until the pixel leaves the cell.
struct Cell {
    key: [u32; 2],
    sampled: [f32; 3],
    amounts: [f32; MOST_INKS],
}

/// `value` rounded to a pixel index no further than `last`.
fn pixel(value: f32, last: usize) -> usize {
    whole(f64::from(value)).min(last)
}

/// The screens of the inks: each turned by `angle` and the ink's own extra angle, and moved by its share of the offset.
fn screens(params: &Params, count: usize) -> Vec<Screen> {
    (0..count)
        .map(|ink| {
            let extra = match params.separation {
                Separation::None => 0.0,
                Separation::Cmyk => CMYK_SCREEN_ANGLES[ink],
                Separation::Spot => SPOT_SCREEN_STEP * float(ink),
            };
            let turn = (f64::from(params.angle) + extra).rem_euclid(360.0).to_radians(); // constant: a full turn in degrees
            Screen {
                sin: cast::single(turn.sin()),
                cos: cast::single(turn.cos()),
                shift: shift(params.offset, params.offset_angle, ink),
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
    }
}

/// The dot screen.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
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
    };
    let count = match params.separation {
        Separation::None => 1,
        Separation::Cmyk => MOST_INKS,
        Separation::Spot => params.inks.len().min(MOST_INKS),
    };
    let screens = screens(params, count);
    let ink_is_bright = luminance(paper) < params.paper_threshold;
    let (inv_cell, inv_feather) = (1.0 / cell, 1.0 / params.feather);
    let mut pixels = vec![[0.0; 3]; height * width];
    par::rows(&mut pixels, width, |first, band| {
        let mut cells: Vec<Cell> = screens
            .iter()
            .map(|_| Cell {
                key: [u32::MAX; 2], // a NaN pattern, which no cell has
                sampled: [0.0; 3],
                amounts: [0.0; MOST_INKS],
            })
            .collect();
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let y = cast::single(float(first + offset));
            for (column, out) in line.iter_mut().enumerate() {
                let x = cast::single(float(column));
                let mut coverages = [0.0; MOST_INKS];
                for (index, (screen, remembered)) in screens.iter().zip(&mut cells).enumerate() {
                    let (sin, cos) = (screen.sin, screen.cos);
                    let (px, py) = (x - screen.shift[0], y - screen.shift[1]);
                    // the pixel in the rotated grid, in cells, and the centre of the cell it belongs to
                    let (u, v) = ((px * cos + py * sin) * inv_cell, (py * cos - px * sin) * inv_cell);
                    let (cu, cv) = (u.floor() + 0.5, v.floor() + 0.5);
                    if remembered.key != [cu.to_bits(), cv.to_bits()] {
                        // the tone at the centre, mapped back to the picture
                        let column = pixel((cu * cos - cv * sin) * cell + screen.shift[0], width - 1);
                        let row = pixel((cu * sin + cv * cos) * cell + screen.shift[1], height - 1);
                        let sampled = smooth[row * width + column];
                        remembered.key = [cu.to_bits(), cv.to_bits()];
                        remembered.sampled = sampled;
                        remembered.amounts = amounts(params, &inks, ink_is_bright, sampled);
                    }
                    let amount = remembered.amounts[if params.separation == Separation::None {
                        0
                    } else {
                        index
                    }];
                    // the distance from the mark's centre, and its radius (half its width): a dot's area follows the
                    // ink and a dot of full ink reaches the corners of its cell; a line of full ink fills its row
                    let (distance, radius) = match params.shape {
                        Shape::Dot => (
                            ((u - cu) * (u - cu) + (v - cv) * (v - cv)).sqrt(),
                            amount.sqrt() * std::f32::consts::FRAC_1_SQRT_2,
                        ),
                        Shape::Line => ((v - cv).abs(), amount * 0.5),
                    };
                    let fade_in = (radius * cell * 2.0 * inv_feather).min(1.0); // constant: the ramp is half a feather on each side
                    coverages[index] = ((radius - distance) * cell * inv_feather + 0.5).clamp(0.0, 1.0) * fade_in; // constant: the ramp is centred on the edge
                }
                *out = if params.separation != Separation::None {
                    inks.print(&coverages)
                } else if coverages[0] > 0.0 {
                    let sampled = cells[0].sampled;
                    let dot = params.ink.map_or_else(|| vivid(sampled, params.ink_floor), |ink| ink.0);
                    [0, 1, 2].map(|channel| paper[channel] * (1.0 - coverages[0]) + dot[channel] * coverages[0])
                } else {
                    paper
                };
            }
        }
    });
    Image::from_pixels(height, width, pixels)
}
