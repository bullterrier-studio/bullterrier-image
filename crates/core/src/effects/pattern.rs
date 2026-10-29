//! `pattern`: stripes, checks, dots or a grid in two colours in place of the picture. What each does is documented once,
//! on the Python effect; its parameters are declared here.

use serde::Deserialize;

use crate::params::{Choice, Color, Params};
use crate::{Image, Picture};

/// The shapes `pattern` can draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Kind {
    /// Parallel bands.
    Stripes,
    /// A chessboard.
    Checks,
    /// A disc in every cell.
    Dots,
    /// Lines along the cell borders.
    Grid,
}

/// A regular pattern of `cell` px, drawn in a foreground colour on a background colour.
#[derive(Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The pattern: 'stripes' (parallel bands, `thickness` wide), 'checks' (a chessboard of `cell` px squares, the
    /// top-left one in the foreground colour), 'dots' (a disc of `radius` in every cell) or 'grid' (lines `thickness`
    /// wide along the cell borders).
    #[param(default = Grid)]
    pub kind: Kind,
    /// The size of one repeat in px: the distance from one stripe to the next, the side of a check, a dot or a grid
    /// square. Tuned for a picture about 480 px high; larger = a coarser pattern.
    #[param(default = 32.0, unit = px, min = 2, max = 512)]
    pub cell: f32,
    /// The direction the pattern varies along, in degrees clockwise from pointing right, about the picture's top-left
    /// corner: at 0 it changes from left to right, so the stripes are vertical and the checks, dots and grid lines are
    /// aligned with the picture's edges; at 90 it changes downward and the stripes are horizontal.
    #[param(default = 0.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// The colour of the stripes, the dark checks, the dots and the grid lines.
    #[param(default = "#ffffff")]
    pub foreground: Color,
    /// The colour between them.
    #[param(default = "#101018")]
    pub background: Color,
    /// Used by 'stripes' and 'grid': the width of a stripe or a grid line as a fraction of `cell`; 1 fills everything
    /// with the foreground colour. Ignored by 'checks' and 'dots'.
    #[param(default = 0.2, unit = fraction, min = 0, max = 1)]
    pub thickness: f32,
    /// Used by 'dots' only: the radius of a dot as a fraction of `cell`; 0.5 touches the neighbours, above 0.71 the
    /// dots overlap into a filled picture. Ignored by 'stripes', 'checks' and 'grid'.
    #[param(default = 0.3, unit = fraction, min = 0, max = 0.75)]
    pub radius: f32,
    /// The width of the soft edge in px, centred on the shape's border and blended linearly: 0 = hard (jagged) edges, 1
    /// = anti-aliased, larger = blurred shapes.
    #[param(default = 1.0, unit = px, min = 0, max = 32)]
    pub feather: f32,
}

/// The pattern, in the picture's size: the picture is used only for its height and width.
#[expect(clippy::cast_precision_loss, reason = "a pixel index is far below the 2^24 an f32 holds exactly")]
pub(crate) fn render(picture: &Picture<'_>, params: &Params) -> Image {
    let (sin, cos) = params.angle.to_radians().sin_cos();
    let cell = params.cell;
    let half_line = params.thickness * cell / 2.0;
    let [foreground, background] = [params.foreground.0, params.background.0];
    picture.map_at(|row, column, _| {
        // the pixel's centre, then its position along the pattern's own axes, in px
        let (x, y) = (column as f32 + 0.5, row as f32 + 0.5);
        let (u, v) = (x * cos + y * sin, y * cos - x * sin);
        let (in_u, in_v) = (u.rem_euclid(cell), v.rem_euclid(cell)); // inside the cell
        let near_u = in_u.min(cell - in_u); // the distance to the nearest border
        let near_v = in_v.min(cell - in_v);
        // the signed distance in px to the foreground's border, positive within it
        let inside = match params.kind {
            Kind::Stripes => half_line - (in_u - cell / 2.0).abs(),
            Kind::Grid => half_line - near_u.min(near_v),
            Kind::Dots => params.radius * cell - (in_u - cell / 2.0).hypot(in_v - cell / 2.0),
            Kind::Checks => {
                let even = ((u / cell).floor() + (v / cell).floor()).rem_euclid(2.0) == 0.0;
                near_u.min(near_v) * if even { 1.0 } else { -1.0 }
            }
        };
        let coverage = if params.feather == 0.0 {
            f32::from(inside > 0.0)
        } else {
            (inside / params.feather + 0.5).clamp(0.0, 1.0)
        };
        [0, 1, 2].map(|channel| background[channel] + (foreground[channel] - background[channel]) * coverage)
    })
}
