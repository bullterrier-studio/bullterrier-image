//! Fitting a picture to a photo: how large it is, where it sits, and whether it repeats.
//!
//! [`place`] turns any picture (an asset: a texture, a logo, a cut-out) into RGBA at the photo's size, transparent where
//! the picture is not, so an effect only has to combine two pictures of one size.

use super::length::{float, whole};
use super::resample::{Filter, Resize};
use crate::Image;
use crate::params::Choice;

/// How a picture is fitted to the photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice)]
pub enum Fit {
    /// Fills the free area keeping the aspect; what hangs over is cropped.
    Cover,
    /// Fits inside the free area keeping the aspect; the rest of the photo is left.
    Contain,
    /// Fills the free area ignoring the aspect.
    Stretch,
    /// Keeps the aspect and is as wide as the photo (times the scale).
    Width,
    /// Is as wide as the photo (times the scale) and repeats over the whole photo.
    Tile,
}

/// Where a picture smaller or larger than the free area sits, or which tile sits there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice)]
pub enum Anchor {
    /// The top left corner.
    TopLeft,
    /// The middle of the top edge.
    Top,
    /// The top right corner.
    TopRight,
    /// The middle of the left edge.
    Left,
    /// The middle.
    Center,
    /// The middle of the right edge.
    Right,
    /// The bottom left corner.
    BottomLeft,
    /// The middle of the bottom edge.
    Bottom,
    /// The bottom right corner.
    BottomRight,
}

impl Anchor {
    /// The share of the free space before the picture, across and down: 0 at the left or top, a half in the middle, 1 at
    /// the right or bottom.
    const fn shares(self) -> (f64, f64) {
        match self {
            Self::TopLeft => (0.0, 0.0),
            Self::Top => (0.5, 0.0),
            Self::TopRight => (1.0, 0.0),
            Self::Left => (0.0, 0.5),
            Self::Center => (0.5, 0.5),
            Self::Right => (1.0, 0.5),
            Self::BottomLeft => (0.0, 1.0),
            Self::Bottom => (0.5, 1.0),
            Self::BottomRight => (1.0, 1.0),
        }
    }
}

/// The filter that sizes a picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice)]
pub enum Resample {
    /// Keeps hard pixels.
    Nearest,
    /// Soft.
    Bilinear,
    /// Sharper.
    Bicubic,
    /// Sharpest, and the best for shrinking.
    Lanczos,
}

impl Resample {
    const fn filter(self) -> Filter {
        match self {
            Self::Nearest => Filter::Nearest,
            Self::Bilinear => Filter::Bilinear,
            Self::Bicubic => Filter::Bicubic,
            Self::Lanczos => Filter::Lanczos,
        }
    }
}

/// How a picture is put on a photo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placement {
    pub(crate) fit: Fit,
    pub(crate) at: Anchor,
    /// The fitted size as a multiple of the size `fit` gives (a finite number above 0).
    pub(crate) scale: f64,
    /// Pixels kept clear on every side (finite, not negative); never more than leaves a pixel of the shorter side.
    pub(crate) margin: f64,
    pub(crate) resample: Resample,
}

/// `asset` fitted to a photo of `height` x `width` pixels: its straight-alpha RGBA, row after row, transparent (all 0)
/// wherever the picture is not.
///
/// The picture is first sized, then multiplied by `scale`, then put in the free space of the photo inside the margin:
/// - `Cover` fills the area inside the margin, keeping the aspect, and what hangs over is cropped; `Contain` fits inside
///   it, keeping the aspect; `Stretch` fills it without keeping the aspect;
/// - `Width` makes the picture as wide as the photo (a logo's `scale` is then a share of the photo's width);
/// - `Tile` is `Width`, repeated over the whole photo with one tile at the anchor.
///
/// `at` says where in the free space a picture smaller or larger than the area sits (the tiles' phase for `Tile`). Colours
/// are weighted by alpha while sizing, so a transparent pixel lends its neighbours no colour. Only the pixels that show
/// are made, so the work and memory follow the photo and the asset, however large the sized picture would be.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "positions are rounded lengths of pictures far below 2^53; a size is at least 1"
)]
pub(crate) fn place(asset: &Image, (height, width): (usize, usize), placement: &Placement) -> Vec<[f32; 4]> {
    let Placement {
        fit,
        at,
        scale,
        margin,
        resample,
    } = *placement;
    let inset = whole(margin).min((height.min(width) - 1) / 2);
    let (area_h, area_w) = (height - 2 * inset, width - 2 * inset);
    let (source_h, source_w) = (asset.height(), asset.width());
    let (size_h, size_w) = if fit == Fit::Stretch {
        (area_h, area_w)
    } else {
        let factor = match fit {
            Fit::Cover => (float(area_w) / float(source_w)).max(float(area_h) / float(source_h)),
            Fit::Contain => (float(area_w) / float(source_w)).min(float(area_h) / float(source_h)),
            Fit::Width | Fit::Tile | Fit::Stretch => float(width) / float(source_w),
        };
        let sized = |source: usize| whole(float(source) * factor * scale).max(1);
        (sized(source_h), sized(source_w))
    };
    let (across, down) = at.shares();
    // a picture larger than the area starts before it, so a position can be negative
    let start = |inset: usize, area: usize, size: usize, share: f64| {
        inset as i64 + ((float(area) - float(size)) * share).round_ties_even() as i64
    };
    let (top, left) = (start(inset, area_h, size_h, down), start(inset, area_w, size_w, across));

    // the row and column of the sized picture shown at each row and column of the photo, none where it is not
    let shown = |length: usize, first: i64, size: usize| -> Vec<Option<usize>> {
        (0..length)
            .map(|place| {
                let from = place as i64 - first;
                if fit == Fit::Tile {
                    Some(from.rem_euclid(size as i64) as usize)
                } else {
                    (0..size as i64).contains(&from).then_some(from as usize)
                }
            })
            .collect()
    };
    let (shown_rows, shown_columns) = (shown(height, top, size_h), shown(width, left, size_w));
    let rows: Vec<usize> = shown_rows.iter().flatten().copied().collect();
    let columns: Vec<usize> = shown_columns.iter().flatten().copied().collect();
    let mut placed = vec![[0.0; 4]; height * width];
    if rows.is_empty() || columns.is_empty() {
        return placed;
    }

    // colours weighted by alpha, sized, and weighted back
    let alphas = asset.alpha();
    let weighted: Vec<[f32; 4]> = asset
        .as_picture()
        .pixels()
        .iter()
        .enumerate()
        .map(|(at, &[red, green, blue])| {
            let alpha = alphas.map_or(1.0, |alphas| alphas[at]);
            [red * alpha, green * alpha, blue * alpha, alpha]
        })
        .collect();
    let sampled =
        Resize::to_size(source_h, source_w, (size_h, size_w), resample.filter()).sample(&weighted, &rows, &columns);
    let on_photo =
        |shown: &[Option<usize>]| -> Vec<usize> { (0..shown.len()).filter(|&place| shown[place].is_some()).collect() };
    let (photo_rows, photo_columns) = (on_photo(&shown_rows), on_photo(&shown_columns));
    for (next_row, &row) in photo_rows.iter().enumerate() {
        let line = &sampled[next_row * columns.len()..][..columns.len()];
        for (next_column, &column) in photo_columns.iter().enumerate() {
            let [red, green, blue, alpha] = line[next_column];
            if alpha > 0.0 {
                let straight = |level: f32| (level / alpha).clamp(0.0, 1.0);
                placed[row * width + column] = [straight(red), straight(green), straight(blue), alpha];
            }
        }
    }
    placed
}
