//! A family of parallel lines laid on a canvas: how far a point lies across them, and how much ink a line of a given
//! width paints on a pixel. `crosshatch` and `engraving` are the users.
//!
//! The canvas is read in pixels with a pixel's corner at whole numbers (its centre at a half). Lines run `angle` degrees
//! clockwise on screen from the x axis, `spacing` apart, the first centred half a spacing from the canvas's top left corner.

/// Parallel lines `spacing` px apart.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Lines {
    sin: f64,
    cos: f64,
    spacing: f64,
}

impl Lines {
    /// The lines running `angle` degrees below the x axis, `spacing` px apart.
    pub(crate) fn new(angle: f64, spacing: f64) -> Self {
        let turn = angle.rem_euclid(360.0).to_radians(); // constant: a full turn in degrees
        let (sin, cos) = turn.sin_cos();
        Self { sin, cos, spacing }
    }

    /// How far the canvas point (`x`, `y`) lies across the lines, in px, from the canvas's corner.
    pub(crate) fn across(&self, x: f64, y: f64) -> f64 {
        y * self.cos - x * self.sin
    }

    /// The ink lines `width` px wide paint on the pixel whose centre lies `across` the lines (0..1): the share of the
    /// pixel, taken as one px across, that a line covers, so an edge is a ramp one px wide and a line narrower than a px
    /// keeps its area. Lines as wide as the spacing cover everything; the mean over `across` is `width / spacing`.
    pub(crate) fn cover(&self, across: f64, width: f64) -> f64 {
        let place = across / self.spacing;
        let off = (place - place.floor() - 0.5) * self.spacing; // constant: lines are centred half a spacing in
        let half = width / 2.0;
        // a pixel one px across, centred `off` from a line, overlaps the line's band [-half, half] by this much
        let overlap = |off: f64| ((off + 0.5).min(half) - (off - 0.5).max(-half)).max(0.0); // constant: half a px each side
        (overlap(off) + overlap(off - self.spacing) + overlap(off + self.spacing)).min(1.0)
    }
}
