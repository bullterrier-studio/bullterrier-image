//! A picture read again and again, zoomed or spun about a centre, and added up: the core of radial blurs and rays.

use super::par;
use super::sample::Sampler;
use crate::params::Choice;

/// How each reading moves the picture about the centre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Choice)]
pub enum Motion {
    /// Turned about the centre: the smear runs along the circle around it, as a rotating camera or wheel would blur.
    Spin,
    /// Scaled about the centre: the smear runs along the ray through it, as zooming the lens during the exposure would.
    Zoom,
}

/// The sum over the `readings` of `weight` times the picture read again about `centre` as `offset` says; each colour
/// channel on its own, an `(offset, weight)` pair each.
///
/// `centre` is the (row, column) of the point everything moves about, in pixels (it may lie outside the picture). A
/// pixel of a reading holds the picture at `centre + M (pixel - centre)`: for `Zoom` M is the scale `1 + offset` (0 reads
/// the picture as it is, -0.5 reads each pixel from half as far from the centre, so the picture is enlarged about it;
/// +1 doubles the distance), for `Spin` M turns the pixel's offset by the angle `offset` in radians, counterclockwise as
/// seen on the picture (+pi/2 carries what lies to the right of the centre to above it). The centre itself never moves.
/// The sampler says how a point between pixels is read and what lies beyond the edge.
///
/// The weights are not normalised or clipped, so the result can leave 0..1 and the caller scales it; with no readings
/// the result is black. Rows are filled in bands on the render's threads, and the result is the same on any number of them.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "a picture side is far below 2**52; the sums are made in f64 and stored in the f32 image"
)]
pub(crate) fn accumulate(
    sampler: &Sampler<'_>,
    motion: Motion,
    centre: (f64, f64),
    readings: &[(f64, f64)],
) -> Vec<[f32; 3]> {
    // reading k puts a pixel's offset (down, across) from the centre through the matrix [[a, b], [c, d]]
    let moves: Vec<([[f64; 2]; 2], f64)> = readings
        .iter()
        .map(|&(offset, weight)| match motion {
            Motion::Zoom => ([[1.0 + offset, 0.0], [0.0, 1.0 + offset]], weight),
            Motion::Spin => ([[offset.cos(), offset.sin()], [-offset.sin(), offset.cos()]], weight),
        })
        .collect();
    let (height, width) = (sampler.height(), sampler.width());
    let mut out = vec![[0.0; 3]; height * width];
    par::rows(&mut out, width, |first_row, band| {
        for (at, pixel) in band.iter_mut().enumerate() {
            let (down, across) = (
                (first_row + at / width) as f64 - centre.0,
                (at % width) as f64 - centre.1,
            );
            let mut sum = [0.0; 3];
            for &([[a, b], [c, d]], weight) in &moves {
                let read = sampler.at(centre.0 + a * down + b * across, centre.1 + c * down + d * across);
                for (total, level) in sum.iter_mut().zip(read) {
                    *total += weight * level;
                }
            }
            *pixel = sum.map(|total| total as f32);
        }
    });
    out
}

#[cfg(test)]
mod tests {
    //! The sum against pictures whose reading can be worked out by hand: a lit pixel carried to a place, ramps, a cubic.
    //! It is private to the core; the effects that use it show only blends of many readings.

    #![expect(clippy::cast_precision_loss, reason = "small picture sizes and test values")]

    use super::{Motion, accumulate};
    use crate::ops::edge::Extension;
    use crate::ops::sample::{Interpolation, Sampler};

    const HEIGHT: usize = 9;
    const WIDTH: usize = 13;
    /// The middle pixel of the 9 x 13 pictures: (row, column).
    const MIDDLE: (f64, f64) = (4.0, 6.0);

    /// A black picture with one pixel lit in one channel.
    fn dot(row: usize, column: usize, channel: usize, level: f32) -> Vec<[f32; 3]> {
        let mut pixels = vec![[0.0; 3]; HEIGHT * WIDTH];
        pixels[row * WIDTH + column][channel] = level;
        pixels
    }

    /// The result of `accumulate` about the middle pixel, reading beyond the edge as the nearest pixel.
    fn run(pixels: &[[f32; 3]], motion: Motion, read: Interpolation, readings: &[(f64, f64)]) -> Vec<[f32; 3]> {
        let sampler = Sampler::new(pixels, HEIGHT, WIDTH, read, Extension::Nearest);
        accumulate(&sampler, motion, MIDDLE, readings)
    }

    /// (row, column) of every pixel of channel 0 above 1e-6.
    fn lit(out: &[[f32; 3]]) -> Vec<(usize, usize)> {
        (0..out.len())
            .filter(|&at| out[at][0] > 1e-6)
            .map(|at| (at / WIDTH, at % WIDTH))
            .collect()
    }

    const NEAREST: Interpolation = Interpolation::Nearest;

    #[test]
    fn zooming_by_less_than_one_moves_the_picture_outward_from_the_centre() {
        // scale 0.5: the dot 2 px right of the centre is read by the pixel 4 px right of it, and its neighbours read half
        // way between two pixels, which the nearest reading rounds up: the dot becomes a patch around that pixel
        let out = run(&dot(4, 8, 0, 1.0), Motion::Zoom, NEAREST, &[(-0.5, 1.0)]);
        assert!((out[4 * WIDTH + 10][0] - 1.0).abs() < f32::EPSILON);
        assert!(out[4 * WIDTH + 8][0].abs() < f32::EPSILON);
        for (row, column) in lit(&out) {
            assert!((3..=5).contains(&row) && (9..=11).contains(&column), "{row}, {column}");
        }
    }

    #[test]
    fn zooming_by_more_than_one_moves_the_picture_toward_the_centre() {
        // scale 2: the dot 4 px out is read by the pixel 2 px out
        let out = run(&dot(4, 10, 0, 1.0), Motion::Zoom, NEAREST, &[(1.0, 1.0)]);
        assert_eq!(lit(&out), [(4, 8)]);
    }

    #[test]
    fn spinning_a_quarter_turn_carries_the_picture_counterclockwise_about_the_centre() {
        // (where the dot is, where it ends up): the right goes up, the left down, the top left, the bottom right
        for (from, to) in [((4, 8), (2, 6)), ((4, 4), (6, 6)), ((2, 6), (4, 4)), ((6, 6), (4, 8))] {
            let out = run(
                &dot(from.0, from.1, 0, 1.0),
                Motion::Spin,
                NEAREST,
                &[(std::f64::consts::FRAC_PI_2, 1.0)],
            );
            assert_eq!(lit(&out), [to], "from {from:?}");
        }
    }

    #[test]
    fn the_centre_pixel_reads_itself_whatever_the_zoom_or_spin() {
        for (motion, offsets) in [(Motion::Zoom, [-0.7, 0.3, 1.5]), (Motion::Spin, [-1.0, 0.4, 2.5])] {
            let readings = offsets.map(|offset| (offset, 1.0));
            let out = run(&dot(4, 6, 0, 1.0), motion, NEAREST, &readings);
            assert!((out[4 * WIDTH + 6][0] - 3.0).abs() < 1e-6, "{motion:?}");
        }
    }

    #[test]
    fn no_movement_and_a_weight_of_one_is_the_picture_and_a_turn_of_two_pi_is_too() {
        let pixels: Vec<[f32; 3]> = (0..HEIGHT * WIDTH)
            .map(|at| [0, 1, 2].map(|channel| (((at * 7 + channel * 31) % 17) as f32) / 17.0))
            .collect();
        for read in [Interpolation::Nearest, Interpolation::Bilinear, Interpolation::Bicubic] {
            for (motion, offset) in [
                (Motion::Zoom, 0.0),
                (Motion::Spin, 0.0),
                (Motion::Spin, std::f64::consts::TAU),
            ] {
                let out = run(&pixels, motion, read, &[(offset, 1.0)]);
                let worst = out
                    .iter()
                    .zip(&pixels)
                    .flat_map(|(a, b)| a.iter().zip(b).map(|(a, b)| (a - b).abs()))
                    .fold(0.0, f32::max);
                assert!(worst < 1e-5, "{motion:?} {offset} {read:?}: {worst}");
            }
        }
    }

    #[test]
    fn the_result_is_the_weighted_sum_of_every_zoomed_picture() {
        // a ramp whose level is the column, read bilinearly at scale s, holds centre + s (column - centre) exactly
        let ramp: Vec<[f32; 3]> = (0..HEIGHT * WIDTH).map(|at| [(at % WIDTH) as f32; 3]).collect();
        let out = run(
            &ramp,
            Motion::Zoom,
            Interpolation::Bilinear,
            &[(-0.5, 0.25), (0.0, 2.0), (0.5, 1.0)],
        );
        for column in 3..10 {
            let c = column as f64;
            let expected = 0.25 * (6.0 + 0.5 * (c - 6.0)) + 2.0 * c + 1.0 * (6.0 + 1.5 * (c - 6.0));
            assert!(
                (f64::from(out[4 * WIDTH + column][1]) - expected).abs() < 1e-4,
                "column {column}"
            );
        }
    }

    #[test]
    fn readings_add_linearly_and_a_zero_weight_drops_its_reading_and_nothing_is_black() {
        let pixels = dot(4, 8, 0, 1.0);
        let (a, b) = ((-0.5, 0.7), (0.4, 1.9));
        let together = run(&pixels, Motion::Zoom, Interpolation::Bilinear, &[a, b]);
        let (first, second) = (
            run(&pixels, Motion::Zoom, Interpolation::Bilinear, &[a]),
            run(&pixels, Motion::Zoom, Interpolation::Bilinear, &[b]),
        );
        for ((sum, one), other) in together.iter().zip(&first).zip(&second) {
            for channel in 0..3 {
                assert!((sum[channel] - one[channel] - other[channel]).abs() < 1e-6);
            }
        }
        let dropped = run(&pixels, Motion::Zoom, NEAREST, &[(-0.5, 0.0), (0.0, 3.0)]);
        assert_eq!(lit(&dropped), [(4, 8)]);
        assert!((dropped[4 * WIDTH + 8][0] - 3.0).abs() < 1e-6);
        assert!(
            run(&pixels, Motion::Zoom, NEAREST, &[])
                .iter()
                .all(|pixel| pixel.iter().all(|level| level.abs() < f32::EPSILON))
        );
    }

    #[test]
    fn each_colour_channel_is_read_on_its_own() {
        let out = run(&dot(4, 8, 1, 0.5), Motion::Zoom, NEAREST, &[(-0.5, 2.0)]);
        assert!((out[4 * WIDTH + 10][1] - 1.0).abs() < 1e-6);
        assert!(out.iter().all(|pixel| pixel[0] == 0.0 && pixel[2] == 0.0));
    }

    #[test]
    fn what_lies_beyond_the_edge_follows_the_edge_rule() {
        // scale 3 about the middle: a pixel in column 0 reads column 6 + 3 (0 - 6) = -12 of a ramp column / 12. 'nearest'
        // repeats the first column (0); 'wrap' reads column -12 + 13 = 1; 'reflect' (d c b a | a b c d) puts column 11 at
        // -12; 'mirror' (d c b | a b c d) puts column 12 there
        let ramp: Vec<[f32; 3]> = (0..HEIGHT * WIDTH).map(|at| [(at % WIDTH) as f32 / 12.0; 3]).collect();
        let table = [
            (Extension::Nearest, 0.0),
            (Extension::Wrap, 1.0 / 12.0),
            (Extension::Reflect, 11.0 / 12.0),
            (Extension::Mirror, 1.0),
            (Extension::Constant(0.25), 0.25),
        ];
        for (rule, expected) in table {
            let sampler = Sampler::new(&ramp, HEIGHT, WIDTH, NEAREST, rule);
            let out = accumulate(&sampler, Motion::Zoom, MIDDLE, &[(2.0, 1.0)]);
            assert!((out[4 * WIDTH][0] - expected).abs() < 1e-6, "{rule:?}");
        }
    }

    #[test]
    fn the_centre_may_lie_outside_the_picture() {
        // centre 10 columns left of the picture, scale 2: column 0 reads column -10 + 2 (0 + 10) = 10
        let ramp: Vec<[f32; 3]> = (0..HEIGHT * WIDTH).map(|at| [(at % WIDTH) as f32; 3]).collect();
        let sampler = Sampler::new(&ramp, HEIGHT, WIDTH, NEAREST, Extension::Nearest);
        let out = accumulate(&sampler, Motion::Zoom, (4.0, -10.0), &[(1.0, 1.0)]);
        assert!((out[4 * WIDTH][0] - 10.0).abs() < 1e-6);
    }

    #[test]
    fn bicubic_reads_a_cubic_exactly_where_bilinear_and_nearest_do_not() {
        // (column / 60)^3 zoomed by 0.5 about column 30 is read at 30 + (column - 30) / 2: the cubic through the neighbours is
        // exact on a cubic away from the edge (to f32 rounding), the straight lines of the bilinear reading are not
        let cubic: Vec<[f32; 3]> = (0..3 * 61).map(|at| [((at % 61) as f32 / 60.0).powi(3); 3]).collect();
        let errors = [Interpolation::Nearest, Interpolation::Bilinear, Interpolation::Bicubic].map(|read| {
            let sampler = Sampler::new(&cubic, 3, 61, read, Extension::Nearest);
            let out = accumulate(&sampler, Motion::Zoom, (1.0, 30.0), &[(-0.5, 1.0)]);
            (20..41_usize)
                .map(|column| {
                    let at = 30.0 + (column as f64 - 30.0) / 2.0;
                    (f64::from(out[61 + column][0]) - (at / 60.0).powi(3)).abs()
                })
                .fold(0.0, f64::max)
        });
        assert!(errors[2] < 5e-6, "bicubic {}", errors[2]);
        assert!(errors[1] > 3e-5, "bilinear {}", errors[1]);
        assert!(errors[0] > errors[1], "nearest {}", errors[0]);
    }
}
