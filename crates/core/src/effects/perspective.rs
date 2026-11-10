//! `perspective`: the picture tilted as a camera sees it: keystoned, skewed, turned and zoomed in place. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::centre::Centre;
use crate::ops::edge::Edge;
use crate::ops::sample::{Interpolation, Sampler};
use crate::params::Params;
use crate::{Canvas, Footprint};
use crate::{Image, Inputs, Picture};

/// Where `perspective` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[],
};

/// What `perspective` reads around a pixel.
pub(crate) fn footprint(_params: &Params, _canvas: &Canvas) -> Footprint {
    Footprint::Whole
}

/// Tilt the picture as a camera would see it: keystone it by `vertical` then `horizontal`, shear, turn and zoom it.
///
/// A perspective correction: a keystone and a skew along each axis, then a turn and a zoom about the centre. The picture is
/// moved by one projective transform about its centre, which keeps straight lines straight: first the vertical keystone
/// (the far edge shrinks to 1 - `vertical` of the near one, the near edge keeps its width), then the horizontal one, then
/// the skew (rows slide sideways by `shear_x` px per row, then columns down by `shear_y` px per column, so the area never
/// changes), then `rotate` degrees clockwise, then `scale`. With all six at 0 the picture is returned unchanged. The output
/// beyond the vanishing line reads from outside the picture, so `edge` says what it shows. The result never leaves 0..1.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The vertical keystone: the fraction by which the far edge shrinks while the near one keeps its width, straight lines
    /// staying straight, as when a camera tilts back to shoot a tall building. Positive makes the top the far side (the top
    /// edge narrows to 1 - vertical of the bottom edge), negative makes the bottom the far side; 0 = none. Raising it makes
    /// a steeper converging; it stops short of 1, where the far edge would shrink to a point.
    #[param(default = 0.25, unit = fraction, min = -0.9, max = 0.9)]
    pub vertical: f32,
    /// The horizontal keystone: the fraction by which the far edge shrinks while the near one keeps its height, as when a
    /// camera turns to the side. Positive makes the right side the far side (the right edge shortens to 1 - horizontal of
    /// the left edge), negative makes the left side the far side; 0 = none. Applied after the vertical keystone.
    #[param(default = 0.0, unit = fraction, min = -0.9, max = 0.9)]
    pub horizontal: f32,
    /// The horizontal skew, after the keystones, as a slope in columns per row: each row slides sideways by this many
    /// pixels for every pixel it lies below the centre (and the other way above it), so a vertical line leans into a line of
    /// this slope. 0 = none, 0.2 = a lean of about 11 degrees, 1 = 45 degrees, positive slides the lower half right and the
    /// upper half left, negative the other way. Applied before `shear_y`.
    #[param(default = 0.0, unit = ratio, min = -2, max = 2)]
    pub shear_x: f32,
    /// The vertical skew, after `shear_x`, as a slope in rows per column: each column slides down by this many pixels for
    /// every pixel it lies right of the centre (and up on the left of it), so a horizontal line tilts into a line of this
    /// slope. 0 = none, positive tilts the right side down, negative the right side up.
    #[param(default = 0.0, unit = ratio, min = -2, max = 2)]
    pub shear_y: f32,
    /// How far the picture is turned about its centre, after the keystones and skews, in degrees: positive turns clockwise,
    /// negative counter-clockwise; 0 = none. The corners it uncovers are filled by `edge`.
    #[param(default = 0.0, unit = degrees, min = -180, max = 180, cyclic)]
    pub rotate: f32,
    /// How much the picture is enlarged about its centre, after the keystones, skews and turn, as a fraction of its size: 0
    /// = unchanged, 0.5 = half as big again, -0.5 = half the size (the empty margin is filled by `edge`). Enlarging it hides
    /// the corners that the keystone and turn uncover.
    #[param(default = 0.0, unit = fraction, min = -0.9, max = 4)]
    pub scale: f32,
    /// How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps every value
    /// exactly as it is but edges turn jagged), 'bilinear' blends the four pixels around it (smooth, slightly soft),
    /// 'bicubic' fits a cubic through the neighbouring pixels (sharpest, a little slower, and may ring beside hard edges).
    #[param(default = Bilinear, advanced)]
    pub interpolation: Interpolation,
    /// What fills the margins the keystone, skew, turn and zoom uncover around the picture: 'reflect' (mirrored, edge pixel
    /// repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated: streaks), 'wrap' (the
    /// opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The least depth a pixel is read at (the centre of the picture has depth 1).
// constant: any floor far under 1 and far over the rounding of the depth keeps the place finite
const VANISHING: f64 = 1e-9;

/// A 3x3 matrix, row by row.
type Matrix = [[f64; 3]; 3];

/// `left` times `right`.
fn product(left: &Matrix, right: &Matrix) -> Matrix {
    std::array::from_fn(|row| std::array::from_fn(|column| (0..3).map(|k| left[row][k] * right[k][column]).sum()))
}

/// The inverse of `matrix`, which is not singular: the transposed cofactors over the determinant.
fn inverse(m: &Matrix) -> Matrix {
    // the cofactor of an entry: the minor of the other two rows and columns, taken in cyclic order, which signs it
    let cofactor = |row: usize, column: usize| {
        // constant: a matrix has 3 rows and columns, so the next two of each are taken modulo 3
        let (r1, r2, c1, c2) = ((row + 1) % 3, (row + 2) % 3, (column + 1) % 3, (column + 2) % 3);
        m[r1][c1] * m[r2][c2] - m[r1][c2] * m[r2][c1]
    };
    let determinant: f64 = (0..3).map(|column| m[0][column] * cofactor(0, column)).sum();
    std::array::from_fn(|row| std::array::from_fn(|column| cofactor(column, row) / determinant))
}

/// The picture tilted.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    // the picture's edges are at the pixels' edges
    #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
    let (half_height, half_width) = (height as f64 / 2.0, width as f64 / 2.0);
    // A keystone is [[a, 0, 0], [0, a, 0], [c, 0, 1]]-like: a point's depth grows linearly across the picture, by the
    // fraction `slope` of the far side's, and `a` keeps the near edge's width.
    let (vertical, horizontal) = (f64::from(params.vertical), f64::from(params.horizontal));
    let slope_v = -vertical / (2.0 - vertical.abs());
    let slope_h = horizontal / (2.0 - horizontal.abs());
    let keystone_v = [
        [1.0 - slope_v.abs(), 0.0, 0.0],
        [0.0, 1.0 - slope_v.abs(), 0.0],
        [0.0, slope_v / half_height, 1.0],
    ];
    let keystone_h = [
        [1.0 - slope_h.abs(), 0.0, 0.0],
        [0.0, 1.0 - slope_h.abs(), 0.0],
        [slope_h / half_width, 0.0, 1.0],
    ];
    let skew_x = [[1.0, f64::from(params.shear_x), 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let skew_y = [[1.0, 0.0, 0.0], [f64::from(params.shear_y), 1.0, 0.0], [0.0, 0.0, 1.0]];
    let (sin, cos) = f64::from(params.rotate).to_radians().sin_cos();
    let turn = [[cos, -sin, 0.0], [sin, cos, 0.0], [0.0, 0.0, 1.0]];
    let zoom_by = 1.0 + f64::from(params.scale);
    let zoom = [[zoom_by, 0.0, 0.0], [0.0, zoom_by, 0.0], [0.0, 0.0, 1.0]];
    // from a point of the output to the point it shows
    let forward = [&turn, &skew_y, &skew_x, &keystone_h, &keystone_v]
        .into_iter()
        .fold(zoom, |so_far, next| product(&so_far, next));
    let back = inverse(&forward);
    let sampler = Sampler::new(
        picture.pixels(),
        height,
        width,
        params.interpolation,
        params.edge.extension(),
    );
    Centre::new(height, width, 0.5, 0.5).warp(&sampler, |spot| {
        let (across, down) = (spot.across, spot.down);
        // a pixel on the vanishing line would read a place at infinity: it reads one far away instead, where the edge rule
        // alone says what shows
        let depth = back[2][0] * across + back[2][1] * down + back[2][2];
        let depth = if depth.abs() < VANISHING {
            VANISHING.copysign(depth)
        } else {
            depth
        };
        spot.place_at(
            (back[1][0] * across + back[1][1] * down + back[1][2]) / depth,
            (back[0][0] * across + back[0][1] * down + back[0][2]) / depth,
        )
    })
}
