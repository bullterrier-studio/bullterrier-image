//! The region kinds laid over a frame, made for the working picture: the frame they are laid over, and the bend of a torn
//! edge.

use super::size;
use crate::ops::cuts::{self, Cut as Laid};
use crate::ops::noise::{self, Tear};
use crate::ops::outlines::{self, Frame, Limits};
use crate::ops::pieces::{self, Bounds};
use crate::ops::{cast, grid, length, raster};
use crate::region::{Cut, Cuts, Edge, EdgeStyle, Grid, Path};
use crate::{Error, Layer, Picture};

/// How a torn edge moves the pixel centres of a picture before a kind classifies them: the shifts in px of the working
/// picture along x and along y, row after row.
pub(super) struct Bend {
    x: Vec<f32>,
    y: Vec<f32>,
}

impl Bend {
    /// The bend of `edge` over `picture`, none if the edge is straight or tears by nothing. The edge's px are design px, so
    /// they are scaled by the picture's size, and its noise is laid on the frame: the same tear at any size.
    pub(super) fn of(edge: &Edge, picture: &Picture<'_>) -> Option<Self> {
        if edge.style == EdgeStyle::Straight || edge.amount == 0.0 {
            return None;
        }
        let (height, width) = (picture.height(), picture.width());
        let ratio = size::ratio_of(picture);
        let long_side = length::float(height.max(width));
        let tear = Tear {
            scale_share: edge.scale * ratio / long_side,
            octaves: edge.octaves,
            persistence: edge.persistence,
            seed: edge.seed,
        };
        let amount = cast::single(edge.amount * ratio);
        let [x, y] = noise::bend(height, width, &tear).map(|mut field| {
            for shift in &mut field {
                *shift *= amount;
            }
            field
        });
        Some(Self { x, y })
    }

    /// The shifts for [`grid::grid`].
    fn shifts(&self) -> [&[f32]; 2] {
        [&self.x, &self.y]
    }
}

/// The rectangle a kind of region `name` is laid over: the bounding box of the selection `fit` (named by `fit_name`), else
/// the whole `picture`.
///
/// # Errors
/// [`Error::Recipe`] if `fit` chooses nothing in this picture.
fn frame(name: &str, fit_name: Option<&str>, fit: Option<&Layer>, picture: &Picture<'_>) -> Result<Bounds, Error> {
    let (height, width) = (picture.height(), picture.width());
    let (Some(fit_name), Some(fit)) = (fit_name, fit) else {
        return Ok(Bounds::whole(height, width));
    };
    pieces::bounds(fit.values(), height, width).ok_or_else(|| {
        Error::Recipe(format!(
            "region '{name}': `fit` '{fit_name}' chooses nothing in this picture, so it has no frame"
        ))
    })
}

/// `bounds` as a frame in px.
fn in_px(bounds: Bounds) -> Frame {
    [bounds.left, bounds.top, bounds.right, bounds.bottom].map(length::float)
}

/// The refusal of the region `name` for the reason `said`.
fn refuse(name: &str, said: &str) -> Error {
    Error::Recipe(format!("region '{name}': {said}"))
}

/// The pieces of `picture` that the grid `spec` (the region `name`) makes, and their number, laid over the box of `fit`
/// (the selection `spec.fit` names) if it has one.
///
/// # Errors
/// As [`frame`].
pub(super) fn grid(
    name: &str,
    spec: &Grid,
    fit: Option<&Layer>,
    picture: &Picture<'_>,
) -> Result<(Vec<i32>, usize), Error> {
    let frame = frame(name, spec.fit.as_deref(), fit, picture)?;
    let bend = Bend::of(&spec.edge, picture);
    Ok(grid::grid(
        picture.height(),
        picture.width(),
        frame,
        &spec.columns.0,
        &spec.rows.0,
        bend.as_ref().map(Bend::shifts),
    ))
}

/// How much of each pixel of `picture` the path `spec` (the region `name`) covers, soft at its border, laid over the box of
/// `fit` (the selection `spec.fit` names) if it has one. A torn edge reads the coverage, bilinearly, at the bent centres.
///
/// # Errors
/// As [`frame`]; [`Error::Recipe`] naming the region if the path data cannot be drawn (see [`outlines::path`]) or would
/// take too long to fill.
pub(super) fn path(name: &str, spec: &Path, fit: Option<&Layer>, picture: &Picture<'_>) -> Result<Vec<f32>, Error> {
    let (height, width) = (picture.height(), picture.width());
    let bounds = frame(name, spec.fit.as_deref(), fit, picture)?;
    let limits = Limits::STANDARD;
    let shape = outlines::path(&spec.d, in_px(bounds), &limits).map_err(|said| refuse(name, &said))?;
    let covered = raster::coverage(height, width, &shape, spec.samples as usize, limits.crossings)
        .map_err(|said| refuse(name, &said))?;
    Ok(match Bend::of(&spec.edge, picture) {
        None => covered,
        Some(bend) => raster::read_bent(&covered, height, width, bend.shifts()),
    })
}

/// The pieces of `picture` that the cuts `spec` (the region `name`) make, and their number, laid over the box of `fit` (the
/// selection `spec.fit` names) if it has one. A torn edge moves the pixel centres before they are sorted by side.
///
/// # Errors
/// As [`frame`]; [`Error::Recipe`] naming the region if a curve's path data cannot be drawn (see [`outlines::path`]) or its
/// side would take too long to paint.
pub(super) fn cuts(
    name: &str,
    spec: &Cuts,
    fit: Option<&Layer>,
    picture: &Picture<'_>,
) -> Result<(Vec<i32>, usize), Error> {
    let frame = in_px(frame(name, spec.fit.as_deref(), fit, picture)?);
    let limits = Limits::STANDARD;
    let lines = spec
        .lines
        .iter()
        .map(|cut| match cut {
            Cut::Line(line) => Ok(Laid::Line {
                x: line.x,
                y: line.y,
                angle: line.angle,
            }),
            Cut::Curve(curve) => outlines::path(&curve.path, frame, &limits).map(Laid::Curve),
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|said| refuse(name, &said))?;
    let bend = Bend::of(&spec.edge, picture);
    cuts::pieces(
        picture.height(),
        picture.width(),
        frame,
        &lines,
        bend.as_ref().map(Bend::shifts),
        limits.crossings,
    )
    .map_err(|said| refuse(name, &said))
}
