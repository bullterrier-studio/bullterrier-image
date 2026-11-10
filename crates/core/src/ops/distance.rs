//! Distance: how far each pixel is from the nearest pixel of a mask, or from the other side of a silhouette.
//!
//! The exact Euclidean distance transform of Felzenszwalb and Huttenlocher ("Distance Transforms of Sampled Functions",
//! 2012): the squared distance is separable, so it is found along each row and then down each column, each pass a lower
//! envelope of parabolas in time linear in the line. Distances are between pixel centres, in px.

use crate::ops::par;

/// The distance from each pixel's centre of the `height` x `width` grid (row after row) to the centre of the nearest
/// pixel where `targets` is true: 0 on a target, 1 beside it, then 1.41, 2 and so on; `f32::INFINITY` everywhere when no
/// pixel is a target. Exact (the squared distances are whole numbers, worked in f64), so the same at any thread count.
/// `targets` holds `height * width` values, both at least 1.
pub(crate) fn to_nearest(targets: &[bool], (height, width): (usize, usize)) -> Vec<f32> {
    // along each row: the squared distance to the nearest target in the same row
    let mut across = vec![0.0; height * width];
    par::rows(&mut across, width, |first_row, band| {
        let mut envelope = Envelope::default();
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let row = &targets[(first_row + offset) * width..][..width];
            envelope.lower(|at| if row[at] { Some(0.0) } else { None }, line);
        }
    });
    // down each column, over the rows' squared distances: the squared distance to the nearest target anywhere
    let mut down = vec![0.0; width * height];
    par::rows(&mut down, height, |first_column, band| {
        let mut envelope = Envelope::default();
        for (offset, line) in band.chunks_exact_mut(height).enumerate() {
            let column = first_column + offset;
            envelope.lower(
                |row| Some(across[row * width + column]).filter(|squared: &f64| squared.is_finite()),
                line,
            );
        }
    });
    let mut distance = vec![0.0; height * width];
    par::rows(&mut distance, width, |first_row, band| {
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let row = first_row + offset;
            for (column, value) in line.iter_mut().enumerate() {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "a distance in px, stored in f32 like a picture"
                )]
                {
                    *value = down[column * height + row].sqrt() as f32;
                }
            }
        }
    });
    distance
}

/// How far every pixel of the `height` x `width` grid is from the other side of the silhouette `inside` (row after row):
/// outside it, the distance to the nearest inside pixel (1 beside it, then 1.41, 2...), inside it minus the distance to
/// the nearest outside pixel (-1 on its outermost pixels). Only the boundary between the two counts: the grid's own
/// border is no edge. A silhouette of no pixel is `+INFINITY` everywhere, one of every pixel `-INFINITY` everywhere.
pub(crate) fn signed(inside: &[bool], dims: (usize, usize)) -> Vec<f32> {
    let outside: Vec<bool> = inside.iter().map(|&within| !within).collect();
    let (to_inside, to_outside) = (to_nearest(inside, dims), to_nearest(&outside, dims));
    inside
        .iter()
        .zip(to_inside.into_iter().zip(to_outside))
        .map(|(&within, (out, inward))| if within { -inward } else { out })
        .collect()
}

/// The lower envelope of the parabolas `(x - site)^2 + height` of the sites of a line, with the room it needs kept
/// between lines.
#[derive(Default)]
struct Envelope {
    /// The parabolas that make up the envelope, left to right: (site, height, where it starts to be the lowest).
    parts: Vec<(f64, f64, f64)>,
}

impl Envelope {
    /// `out[x]` = the least `(x - site)^2 + height(site)` over the positions `site` of the line where `height` is some
    /// (finite) value, `f64::INFINITY` at every `x` if there is none. `out` is as long as the line.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a position in a picture side, far below 2**52"
    )]
    fn lower(&mut self, height: impl Fn(usize) -> Option<f64>, out: &mut [f64]) {
        self.parts.clear();
        for site in 0..out.len() {
            let Some(rise) = height(site) else { continue };
            let at = site as f64;
            // drop the parabolas this one lies below from where they start to be the lowest; the first never is
            let mut start = f64::NEG_INFINITY;
            while let Some(&(before, below, from)) = self.parts.last() {
                start = ((rise + at * at) - (below + before * before)) / (2.0 * (at - before));
                if start > from {
                    break;
                }
                self.parts.pop();
                start = f64::NEG_INFINITY;
            }
            self.parts.push((at, rise, start));
        }
        if self.parts.is_empty() {
            out.fill(f64::INFINITY);
            return;
        }
        let mut lowest = 0; // constant: the walk starts at the leftmost parabola
        for (x, value) in out.iter_mut().enumerate() {
            let at = x as f64;
            while self.parts.get(lowest + 1).is_some_and(|&(_, _, from)| from < at) {
                lowest += 1;
            }
            let (site, rise, _) = self.parts[lowest];
            *value = (at - site) * (at - site) + rise;
        }
    }
}

#[cfg(test)]
mod tests {
    //! Every pixel's distance against a brute-force search over every pixel of small random masks. The effects that
    //! use it see the distance only through a coverage ramp one pixel wide, so the exact distance of every pixel is
    //! checked here, next to the op.

    use super::{signed, to_nearest};

    /// `count` booleans, each true with a chance of about `share` in 256, from a hash of their place and `seed`.
    fn mask(count: usize, seed: u32, share: u32) -> Vec<bool> {
        (0..count)
            .map(|index| {
                let mut hash =
                    u32::try_from(index).unwrap_or(0).wrapping_mul(0x9e37_79b9) ^ seed.wrapping_mul(0x85eb_ca6b);
                hash = (hash ^ (hash >> 15)).wrapping_mul(0x2c1b_3c6d);
                hash = (hash ^ (hash >> 12)).wrapping_mul(0x297a_2d39);
                (hash ^ (hash >> 15)) % 256 < share
            })
            .collect()
    }

    /// The distance from each pixel to the nearest pixel where `targets` is true, by trying every pixel.
    fn brute(targets: &[bool], height: usize, width: usize) -> Vec<f64> {
        (0..height * width)
            .map(|at| {
                (0..height * width)
                    .filter(|&other| targets[other])
                    .map(|other| {
                        let dy = (at / width).abs_diff(other / width);
                        let dx = (at % width).abs_diff(other % width);
                        f64::from(u32::try_from(dy * dy + dx * dx).unwrap_or(u32::MAX)).sqrt()
                    })
                    .fold(f64::INFINITY, f64::min)
            })
            .collect()
    }

    /// The shapes tried: single rows and columns, a single pixel, squares and oblongs both ways.
    const SHAPES: [(usize, usize); 7] = [(1, 1), (1, 9), (9, 1), (7, 7), (5, 11), (11, 4), (13, 13)];

    #[test]
    fn the_distance_to_the_nearest_target_is_the_brute_force_one_at_every_pixel() {
        for (height, width) in SHAPES {
            for seed in 0..40 {
                // from a lone target or two to a mask almost all target
                let share = [3, 20, 90, 240][seed as usize % 4];
                let targets = mask(height * width, seed, share);
                let found = to_nearest(&targets, (height, width));
                let expected = brute(&targets, height, width);
                for (at, (&got, &wanted)) in found.iter().zip(&expected).enumerate() {
                    assert!(
                        (f64::from(got) - wanted).abs() < 1e-5 || got.is_infinite() && wanted.is_infinite(),
                        "{height} x {width}, seed {seed}, pixel {at}: {got}, brute force {wanted}"
                    );
                }
            }
        }
    }

    #[test]
    fn no_target_is_infinitely_far_and_every_target_is_at_0() {
        assert!(
            to_nearest(&[false; 12], (3, 4))
                .iter()
                .all(|value| *value == f32::INFINITY)
        );
        assert!(to_nearest(&[true; 12], (3, 4)).iter().all(|value| *value == 0.0));
    }

    #[test]
    fn the_signed_distance_is_to_the_other_side_positive_outside_and_negative_inside() {
        for (height, width) in SHAPES {
            for seed in 0..12 {
                let inside = mask(height * width, seed, 128);
                let outside: Vec<bool> = inside.iter().map(|within| !within).collect();
                let (to_inside, to_outside) = (brute(&inside, height, width), brute(&outside, height, width));
                for (at, &got) in signed(&inside, (height, width)).iter().enumerate() {
                    let wanted = if inside[at] { -to_outside[at] } else { to_inside[at] };
                    assert!(
                        (f64::from(got) - wanted).abs() < 1e-5
                            || got.is_infinite()
                                && wanted.is_infinite()
                                && got.is_sign_negative() == wanted.is_sign_negative(),
                        "{height} x {width}, seed {seed}, pixel {at}: {got}, brute force {wanted}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_silhouette_of_no_pixel_is_infinitely_far_outside_and_one_of_every_pixel_infinitely_far_inside() {
        assert!(signed(&[false; 6], (2, 3)).iter().all(|value| *value == f32::INFINITY));
        assert!(
            signed(&[true; 6], (2, 3))
                .iter()
                .all(|value| *value == f32::NEG_INFINITY)
        );
        // a pixel beside the other side is 1 from it, and the picture's border is no edge
        assert_eq!(signed(&[true, true, false], (1, 3)), [-2.0, -1.0, 1.0]);
    }
}
