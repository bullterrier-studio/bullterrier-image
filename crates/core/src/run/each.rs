//! An `each` step: a look for each piece of a partition, blended across the pieces' borders.

use super::regions::Evaluator;
use super::step::{Laid, StepRun};
use crate::ops::blur;
use crate::ops::pieces::piece;
use crate::ops::random::Rng;
use crate::registry::Registry;
use crate::{Assign, Each, Error, Image, Picture, Presets};

/// An `each` step ready to run.
pub(super) struct EachRun {
    /// The name of the partition region whose pieces get a look each.
    partition: String,
    /// The looks, a step each or none for a piece left as it is.
    looks: Vec<Option<StepRun>>,
    assign: Assign,
    seed: u32,
    /// How softly a look fades into the next across a border, in design pixels.
    feather: f64,
    laid: Laid,
}

/// The look (an index into the looks, `looks` of them) of each of `pieces` pieces, piece 0 first.
fn assignment(assign: Assign, seed: u32, looks: usize, pieces: usize) -> Vec<usize> {
    match assign {
        Assign::Cycle => (0..pieces).map(|piece| piece % looks).collect(),
        Assign::Random => {
            let mut rng = Rng::seeded(seed);
            (0..pieces).map(|_| rng.below(looks)).collect()
        }
    }
}

impl EachRun {
    /// The each step `each` (at `place`, as `step 2`) with its looks resolved (at `step 2, look 1`).
    pub(super) fn new(
        registry: &Registry,
        presets: &Presets,
        ratio: f64,
        each: &Each,
        place: &str,
    ) -> Result<Self, Error> {
        let looks = each
            .looks
            .iter()
            .enumerate()
            .map(|(index, look)| {
                look.as_ref()
                    .map(|step| StepRun::new(registry, presets, ratio, step, &format!("{place}, look {}", index + 1)))
                    .transpose()
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            partition: each.each.clone(),
            looks,
            assign: each.assign,
            seed: each.seed,
            feather: each.feather,
            laid: Laid::new(each.opacity.unwrap_or(1.0), each.blend_mode, each.mask.as_ref()),
        })
    }

    /// `before` with each piece of the partition replaced by its look, and the result laid over `before`.
    ///
    /// Every look that has a piece is run once over the whole picture and shown by the weights of its pieces: 1 inside
    /// them, 0 outside, blurred by a Gaussian of `feather` design pixels across the borders (three box blurs that stand for it, whose cost does
    /// not grow with the feather). The weights of all the looks
    /// add up to 1 with the share of the pieces left as they were (a `None` look), so a pixel is
    /// `before + sum of weight x (look - before)`: where a look's pieces are far from every border that look alone.
    /// A look no piece has is not run. Only one look's weights and result are held at a time.
    pub(super) fn apply(
        &self,
        before: &Picture<'_>,
        ratio: f64,
        regions: &mut Evaluator,
        working: &Picture<'_>,
    ) -> Result<Image, Error> {
        let pieces = regions.pieces(&self.partition, working)?;
        let given = assignment(self.assign, self.seed, self.looks.len(), pieces.pieces());
        let (height, width) = (before.height(), before.width());
        let mut out = before.pixels().to_vec();
        for (index, look) in self.looks.iter().enumerate() {
            let Some(look) = look else { continue };
            if !given.contains(&index) {
                continue;
            }
            let mut weights: Vec<f32> = pieces
                .values()
                .iter()
                .map(|&label| f32::from(u8::from(given[piece(label)] == index)))
                .collect();
            blur::gaussian(&mut weights, height, width, self.feather * ratio);
            let result = look.apply(before, ratio, regions, working)?;
            for (((out, was), now), weight) in out
                .iter_mut()
                .zip(before.pixels())
                .zip(result.as_picture().pixels())
                .zip(&weights)
            {
                for channel in 0..3 {
                    out[channel] += weight * (now[channel] - was[channel]);
                }
            }
        }
        // the weights add up to 1 and each look is within 0..1, so only rounding can leave the range
        for pixel in &mut out {
            *pixel = pixel.map(|value| value.clamp(0.0, 1.0));
        }
        self.laid
            .over(before, Image::from_pixels(height, width, out), regions, working)
    }
}
