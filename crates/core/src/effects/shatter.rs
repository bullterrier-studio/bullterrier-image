//! `shatter`: the picture broken into pieces, each moved, turned and flattened, with lines between them. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::edge::Edge;
use crate::ops::length;
use crate::ops::nearest::Buckets;
use crate::ops::pieces::{line_coverage, piece};
use crate::ops::random::Rng;
use crate::ops::raster::centre;
use crate::ops::sample::{Interpolation, Sampler};
use crate::ops::sites::{detail_weights, scatter};
use crate::ops::triangles::{labels, triangles};
use crate::params::{Choice, Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `shatter` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[],
};

/// What `shatter` reads around a pixel: a piece reads the picture where its move and turn carry it, anywhere in it.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// How far `shatter` paints beyond the picture's edge: not worked out.
pub(crate) fn spread(_params: &Params) -> crate::Spread {
    crate::Spread::Unbounded
}

/// The shape of the pieces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice)]
pub enum Shape {
    /// The cells round the points, as broken glass.
    Voronoi,
    /// The Delaunay triangles of the points and of points added round the picture's edge, as in a low-poly picture.
    Triangles,
}

/// Break the picture into pieces (Voronoi cells or triangles), move and turn each, optionally flatten each to its mean colour
/// (stained glass, low-poly), and draw `gap` px lines between them.
///
/// Broken glass: pieces, each showing the picture a little moved and turned (or flattened to one colour), divided by thin
/// lines. The pieces are the Voronoi cells round `pieces` points (random, or gathered along the picture's edges by `detail`),
/// or the triangles of those points and of points round the picture's edge (`shape`). Each draws its own move (up to `offset`
/// px each way) and its own turn (up to `angle` degrees, about its centre) from `seed`, and shows the picture moved and
/// turned that way: one rigid move of the input per piece, so the picture inside a piece is not bent, only cut along the
/// borders. `fill` then pulls each piece's colours to their average. The lines of `color` are drawn along the borders of the
/// pieces where they lie, over the pictures.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How many points the picture breaks round, 4 to 500: with `shape` 'voronoi' each is a piece (the cell nearer to it than
    /// to any other, so the average piece is the picture's area divided by this); with 'triangles' the points are corners of
    /// triangles, and there are more pieces than points (see `shape`). More = smaller pieces.
    #[param(default = 24, unit = count, min = 4, max = 500)]
    pub pieces: usize,
    /// The pieces' shape. 'voronoi' = the cells round the points, as broken glass; 'triangles' = the Delaunay triangles of
    /// the points and of points added round the picture's edge, about as far apart as the others are (about 4 times the
    /// square root of `pieces` of them), as in a low-poly picture. That makes 2 x `pieces` + b - 2 triangles for b edge
    /// points: about 2.75 per point at 24 points on a 4:3 picture, fewer per point as `pieces` grows.
    #[param(default = Voronoi)]
    pub shape: Shape,
    /// How far the points follow the picture's detail: 0 = they lie anywhere at random; 1 = they gather along its edges, in
    /// proportion to how sharply the brightness changes, so edges break into small pieces and plain areas into large ones. In
    /// between mixes the two. A picture with no edges gets random points whatever this is.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1)]
    pub detail: f32,
    /// How far each piece is flattened to one colour, the average of what it shows: 0 = each piece shows the picture (moved
    /// and turned), as it is; 1 = each piece is one flat colour, as stained glass or low-poly; in between mixes the two.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1)]
    pub fill: f32,
    /// The farthest a piece's picture is moved, in pixels, up or down and left or right: each piece draws its own random move
    /// up to this far in each direction. 0 = no piece moves. Raising it tears the picture further apart. Tuned for a picture
    /// about 480 px high.
    #[param(default = 10.0, unit = px, min = 0, max = 200)]
    pub offset: f32,
    /// The most a piece's picture is turned, in degrees, about the piece's own centre: each piece draws its own random turn up
    /// to this far clockwise or anticlockwise. 0 = no piece turns.
    #[param(default = 5.0, unit = degrees, min = 0, max = 45)]
    pub angle: f32,
    /// The width of the lines between the pieces, in pixels, with anti-aliased edges; 0 = no lines, the pieces simply meet
    /// (below 1 px the lines get fainter). Raising it draws the cracks in `color`. Tuned for a picture about 480 px high.
    #[param(default = 0.0, unit = px, min = 0, max = 32)]
    pub gap: f32,
    /// The colour of the lines between the pieces.
    #[param(default = "#000000")]
    pub color: Color,
    /// Random seed for where the pieces lie and how each moves and turns: the same seed always gives the same break, another
    /// seed a different one.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
    /// How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps every value
    /// exactly as it is but turned pieces turn jagged), 'bilinear' blends the four pixels around it (smooth, slightly soft),
    /// 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest, a little slower, and may ring beside hard
    /// edges).
    #[param(default = Bilinear, advanced)]
    pub interpolation: Interpolation,
    /// What lies beyond the picture's edge, for a moved piece that reads from there and for the edges `detail` looks for at
    /// the border (with 'wrap' a mismatch between opposite sides counts as an edge): 'reflect' (the default, as for warps:
    /// mirrored, edge pixel repeated, so a moved piece shows no gap at the border), 'mirror' (mirrored about the edge pixel),
    /// 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// A level (0..1) as the picture holds it.
#[expect(clippy::cast_possible_truncation, reason = "a level in 0..1 fits a float")]
const fn narrow(level: f64) -> f32 {
    level as f32
}

/// The piece number `index` as a label.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "at most a few thousand pieces"
)]
const fn label(index: usize) -> i32 {
    index as i32
}

/// The picture broken into pieces.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let mut rng = Rng::seeded(params.seed);
    let weights = (params.detail > 0.0)
        .then(|| detail_weights(picture.pixels(), (height, width), f64::from(params.detail), params.edge));
    let pieces = params.pieces;
    // (x, y) in px from the top left corner
    let points = scatter(pieces, (height, width), weights.as_deref(), &mut rng);
    // each pixel's piece, and each piece's centre in pixel indices, where a pixel's centre is a whole number
    let (owner, centres): (Vec<i32>, Vec<[f64; 2]>) = match params.shape {
        Shape::Voronoi => {
            let buckets = Buckets::new(&points);
            let owner = (0..height * width)
                .map(|at| {
                    let [x, y] = centre(at, width, None);
                    label(buckets.nearest(x, y))
                })
                .collect();
            (owner, points.iter().map(|p| [p[0] - 0.5, p[1] - 0.5]).collect())
        }
        Shape::Triangles => {
            let spacing = (length::float(height * width) / length::float(pieces)).sqrt();
            let mesh = triangles(height, width, &points, spacing);
            let centres = mesh
                .iter()
                .map(|corners| {
                    // the average of a triangle's corners
                    let sum = corners.iter().fold([0.0, 0.0], |sum, p| [sum[0] + p[0], sum[1] + p[1]]);
                    [sum[0] / 3.0 - 0.5, sum[1] / 3.0 - 0.5] // constant: the mean of a triangle's three corners
                })
                .collect();
            (labels(height, width, &mesh), centres)
        }
    };
    // each piece's share, -1..1, of its move along x and y and of its turn
    let shares: Vec<[f64; 3]> = centres
        .iter()
        .map(|_| [0, 1, 2].map(|_| rng.uniform() * 2.0 - 1.0)) // constant: from 0..1 to -1..1
        .collect();
    let (offset, turn) = (f64::from(params.offset), f64::from(params.angle).to_radians());
    let sampler = Sampler::new(
        picture.pixels(),
        height,
        width,
        params.interpolation,
        params.edge.extension(),
    );
    // a pixel of a piece reads the picture at the point that the piece's move and turn carry to it: take it back by the move,
    // then back round the piece's centre
    let shown = picture.map_at(|row, column, _| {
        let at = piece(owner[row * width + column]);
        let (middle, share) = (centres[at], shares[at]);
        let moved = [middle[0] + share[0] * offset, middle[1] + share[1] * offset];
        let (across, down) = (length::float(column) - moved[0], length::float(row) - moved[1]);
        let (sin, cos) = (turn * share[2]).sin_cos();
        sampler
            .at(
                middle[1] - across * sin + down * cos,
                middle[0] + across * cos + down * sin,
            )
            .map(|level| narrow(level.clamp(0.0, 1.0)))
    });
    let shown = shown.as_picture();
    // at fill 0 the pieces keep the picture they show, and the averages are not needed
    let averages = (params.fill > 0.0).then(|| {
        let mut sums = vec![[0.0_f64; 3]; centres.len()];
        let mut sizes = vec![0_u32; centres.len()];
        for (pixel, &label) in shown.pixels().iter().zip(&owner) {
            let at = piece(label);
            sizes[at] += 1;
            for (total, level) in sums[at].iter_mut().zip(pixel) {
                *total += f64::from(*level);
            }
        }
        // a piece without a pixel is unused
        sums.iter()
            .zip(&sizes)
            .map(|(sum, size)| sum.map(|total| narrow(total / f64::from((*size).max(1)))))
            .collect::<Vec<[f32; 3]>>()
    });
    let cover = line_coverage(&owner, (height, width), params.gap);
    let (fill, tint) = (params.fill, params.color.0);
    shown.map_at(|row, column, pixel| {
        let at = row * width + column;
        let flat = averages.as_ref().map_or(pixel, |all| {
            let average = all[piece(owner[at])];
            [0, 1, 2].map(|channel| pixel[channel] * (1.0 - fill) + average[channel] * fill)
        });
        [0, 1, 2].map(|channel| flat[channel] + (tint[channel] - flat[channel]) * cover[at])
    })
}
