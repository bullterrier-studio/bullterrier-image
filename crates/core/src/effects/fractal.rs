//! `fractal`: the Mandelbrot set or a Julia set, coloured by smooth escape time, in place of the picture. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Input, Meta};
use crate::ops::cast::single;
use crate::ops::length;
use crate::ops::ramp;
use crate::params::{Choice, Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `fractal` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Backgrounds,
    opacity: 1.0,
    inputs: &[Input::Place],
};

/// What `fractal` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Pixel
}

/// The escape radius. Any radius above 2 decides escape correctly; a large one makes the smooth count
/// `n + 1 - log2(log|z| / log R)` continuous across iteration boundaries (Linas Vepstas, "Renormalizing the Mandelbrot
/// Escape").
const ESCAPE_RADIUS: f32 = 256.0; // constant: any radius above 2 decides escape; a large one smooths the count
/// 4 is the diameter of the disc |z| <= 2, which holds the Mandelbrot set and every Julia set; zoom 1 fits it.
const VIEW_SPAN: f64 = 4.0; // constant: the diameter of the disc that holds every Julia set and the Mandelbrot set

/// The sets `fractal` can draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Kind {
    /// Every point is the constant c, z starts at 0.
    Mandelbrot,
    /// Every point is the start z, with the one constant `c_real` + i `c_imag`.
    Julia,
}

/// Replace the picture with the Mandelbrot set or a Julia set, coloured by how fast each point escapes.
///
/// An escape-time fractal: the plane's points z (or c) iterated through z -> z^2 + c until they run away. A generated
/// background: the input is used only for its height and width. A point escapes when |z| exceeds 256 after n steps; its
/// colour is then `colors` at the smooth escape count n + 1 - log2(log|z| / log 256) over `period` (a continuous count,
/// so no steps between bands), and a point that has not escaped after `iterations` steps takes `interior`. The orbits
/// are iterated in single precision.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Which set: 'mandelbrot' (every point is the constant c, z starts at 0; centre it with x = -0.5 to see the whole
    /// set) or 'julia' (every point is the start z, with the one constant `c_real` + i `c_imag`).
    #[param(default = Julia)]
    pub kind: Kind,
    /// Magnification. At 1 the picture's shorter side shows 4 units of the complex plane, enough for the whole
    /// Mandelbrot set or any Julia set; 2 shows 2 units.
    #[param(default = 1.0, unit = ratio, min = 0.1, max = 10000)]
    pub zoom: f32,
    /// The real part of the point at the centre of the picture, in units of the complex plane. 0 centres a Julia set;
    /// -0.5 centres the Mandelbrot set.
    #[param(default = 0.0, unit = ratio, min = -2, max = 2)]
    pub x: f32,
    /// The imaginary part of the point at the centre of the picture, in units of the complex plane; positive moves the
    /// view up.
    #[param(default = 0.0, unit = ratio, min = -2, max = 2)]
    pub y: f32,
    /// Julia only: the real part of the constant c in z -> z^2 + c. It decides the shape: a point inside the Mandelbrot
    /// set gives a connected Julia set, one outside a dust. Ignored by 'mandelbrot'.
    #[param(default = -0.8, unit = ratio, min = -2, max = 2)]
    pub c_real: f32,
    /// Julia only: the imaginary part of the constant c. Ignored by 'mandelbrot'.
    #[param(default = 0.156, unit = ratio, min = -2, max = 2)]
    pub c_imag: f32,
    /// How many times z -> z^2 + c is applied before a point is taken to stay for ever (the interior). More reveals
    /// finer detail at the set's edge and costs time in proportion; zooming in needs more.
    #[param(default = 64, unit = count, min = 1, max = 2000)]
    pub iterations: i32,
    /// How many iterations of escape time take the colours from the first to the last of `colors`; they then run
    /// back, so the bands cycle without a seam. Smaller = more, thinner bands.
    #[param(default = 24.0, unit = count, min = 1, max = 1000, advanced)]
    pub period: f32,
    /// The colours of the bands, evenly spaced; at least two. Points that escape early (far from the set) take the first
    /// colours.
    #[param(default = ["#12002b", "#6a1bff", "#ff2bd6", "#ffd84a"], min_length = 2)]
    pub colors: Vec<Color>,
    /// The colour of the points that never escape: the inside of the set.
    #[param(default = "#000000", advanced)]
    pub interior: Color,
}

/// The smooth escape count of the orbit of `z` under z -> z^2 + c, or none if it has not escaped after `iterations`
/// steps. Single precision, as the orbit is only a picture.
fn escape_count(mut z: (f32, f32), c: (f32, f32), iterations: i32) -> Option<f64> {
    for n in 1..=iterations {
        z = (z.0 * z.0 - z.1 * z.1 + c.0, 2.0 * z.0 * z.1 + c.1);
        let squared = z.0 * z.0 + z.1 * z.1;
        if squared > ESCAPE_RADIUS * ESCAPE_RADIUS {
            let log_radius = 0.5 * f64::from(squared).ln(); // log |z|
            return Some(f64::from(n) + 1.0 - (log_radius / f64::from(ESCAPE_RADIUS).ln()).log2());
        }
    }
    None
}

/// Where the plane's points lie on the canvas: the point of a column and of a canvas row, as the single precision numbers the orbit
/// starts from (the CPU's, to the bit: the orbit is chaotic, so a point a rounding off may escape at another step).
struct View {
    centre: (f64, f64),
    half: (f64, f64),
    step: f64,
}

impl View {
    fn of(place: crate::Place, params: &Params) -> Self {
        let (height, width) = (place.canvas_height, place.canvas_width);
        Self {
            // plane units per pixel; the picture's centre is the point (x, y), rows run downwards and the imaginary axis up
            step: VIEW_SPAN / f64::from(params.zoom) / length::float(height.min(width)),
            centre: (f64::from(params.x), f64::from(params.y)),
            half: (length::float(width - 1) / 2.0, length::float(height - 1) / 2.0),
        }
    }

    fn real(&self, column: usize) -> f32 {
        single(self.centre.0 + (length::float(column) - self.half.0) * self.step)
    }

    fn imaginary(&self, canvas_row: usize) -> f32 {
        single(self.centre.1 - (length::float(canvas_row) - self.half.1) * self.step)
    }
}

/// The fractal, in the picture's size: the picture is used only for its height and width.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    // the view is centred on the canvas, of which the picture holds the rows from `first_row`
    let place = inputs.place(picture);
    let view = View::of(place, params);
    let constant = (params.c_real, params.c_imag);
    let stops: Vec<[f32; 3]> = params.colors.iter().map(|colour| colour.0).collect();
    let period = f64::from(params.period);
    picture.map_at(|row, column, _| {
        let point = (view.real(column), view.imaginary(place.first_row + row));
        let count = match params.kind {
            Kind::Julia => escape_count(point, constant, params.iterations),
            Kind::Mandelbrot => escape_count((0.0, 0.0), point, params.iterations),
        };
        count.map_or(params.interior.0, |count| {
            // forward and back along the colours
            let position = 1.0 - (1.0 - (count / period).rem_euclid(2.0)).abs();
            ramp::at(&stops, single(position))
        })
    })
}

/// The fractal on the GPU (`fractal.wgsl`): the orbit is the CPU's, step for step in single precision (each product rounded on its
/// own, `times`), from the very same start points (a table: one for each column and each canvas row), so a point escapes at the
/// same step. The smooth count and the colour are worked in f32 and agree to its precision: about a ten-thousandth of a level
/// for a point that escapes after 2000 steps at a period of 1.
#[cfg(feature = "gpu")]
pub(crate) fn kernel() -> crate::gpu::Kernel<Params> {
    crate::gpu::Kernel::new(include_str!("fractal.wgsl"), |params: &Params, _: &crate::gpu::Header| {
        let [red, green, blue] = params.interior.0;
        vec![
            [params.c_real, params.c_imag, params.period, 0.0],
            [red, green, blue, 0.0],
            [
                f32::from_bits(params.iterations.cast_unsigned()),
                f32::from_bits(u32::from(params.kind == Kind::Julia)),
                f32::from_bits(u32::try_from(params.colors.len()).unwrap_or(2)),
                0.0,
            ],
        ]
    })
    .with_table(|params: &Params, header: &crate::gpu::Header| {
        let place = header.place();
        let view = View::of(place, params);
        let mut table: Vec<f32> = (0..place.canvas_width).map(|column| view.real(column)).collect();
        table.extend((0..place.canvas_height).map(|row| view.imaginary(row)));
        table.extend(params.colors.iter().flat_map(|colour| colour.0));
        table
    })
}
