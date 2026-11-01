//! A drawing painted on a picture: the pieces its shapes make.

use std::collections::HashMap;

use super::Drawing;
use crate::ops::outlines::Limits;
use crate::ops::{pieces, raster};

/// The pieces that each `id` of a drawing names, in the order the ids first appear. A shape fully covered by later ones keeps
/// its id with no piece; two shapes with one id name the pieces of both.
pub(crate) type Ids = Vec<(String, Vec<usize>)>;

/// A drawing painted on a picture.
pub(crate) struct Pieces {
    /// The piece of each pixel, row after row, numbered by first pixel.
    pub(crate) labels: Vec<i32>,
    /// The number of pieces.
    pub(crate) count: usize,
    /// The pieces of each `id`.
    pub(crate) ids: Ids,
}

impl Drawing {
    /// The pieces of a `height` x `width` picture that the shapes make: each shape back to front, later ones on top, and the
    /// area outside all of them one more piece. With `shift` (the shifts of the pixel centres along x and along y, row after
    /// row) each pixel takes the piece at its moved centre, so the borders wander.
    ///
    /// # Errors
    /// A message if painting would take more crossings of an edge with a scanline, or more pixels, than `limits` allow, before
    /// any is painted.
    pub(crate) fn pieces(
        &self,
        height: usize,
        width: usize,
        shift: Option<[&[f32]; 2]>,
        limits: &Limits,
    ) -> Result<Pieces, String> {
        let (mut crossings, mut painted) = (0_u64, 0_u64);
        for shape in &self.shapes {
            crossings = crossings.saturating_add(raster::crossings(&shape.outlines, height, 1));
            painted = painted.saturating_add(raster::box_pixels(&shape.outlines, height, width));
        }
        if crossings > limits.crossings {
            return Err(format!(
                "painting would take {crossings} crossings of the edges with scanlines, over the limit of {}: use fewer corners",
                limits.crossings
            ));
        }
        if painted > limits.painted {
            return Err(format!(
                "painting would set {painted} pixels, over the limit of {}: use fewer or smaller shapes",
                limits.painted
            ));
        }
        let mut drawn = vec![0_i32; height * width];
        for (number, shape) in (1_i32..).zip(&self.shapes) {
            raster::fill(&mut drawn, height, width, &shape.outlines, shape.rule, number);
        }
        let drawn = match shift {
            Some(shift) => raster::read_floor(&drawn, height, width, shift),
            None => drawn,
        };
        let (labels, count) =
            pieces::renumber(self.shapes.len() + 1, drawn.iter().map(|&number| pieces::piece(number)));
        // the shape (from 1; 0 for none) that each piece shows
        let mut shape_of = vec![0_usize; count];
        for (&number, &label) in drawn.iter().zip(&labels) {
            shape_of[pieces::piece(label)] = pieces::piece(number);
        }
        let mut ids: Ids = Vec::new();
        let mut at: HashMap<&str, usize> = HashMap::new();
        for shape in self.shapes.iter().filter(|shape| !shape.id.is_empty()) {
            at.entry(&shape.id).or_insert_with(|| {
                ids.push((shape.id.clone(), Vec::new()));
                ids.len() - 1
            });
        }
        for (piece, number) in shape_of.into_iter().enumerate().filter(|&(_, number)| number > 0) {
            if let Some(&slot) = at.get(self.shapes[number - 1].id.as_str()) {
                ids[slot].1.push(piece);
            }
        }
        Ok(Pieces { labels, count, ids })
    }
}
