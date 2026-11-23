//! The reductions of a picture on the device against the CPU's own sums, extremes and bins, at sizes that are and are not
//! multiples of a workgroup (256 x 64 pixels): the answer is the CPU's within the stated bound.

use bullterrier_image::{MOST_BINS, Picture, Plane, Reduction};
use pollster::block_on;

use super::gpu;
use super::parity::seeded;

/// Sizes around a workgroup's chunk of 16384 pixels, and below it.
const SIZES: [(usize, usize); 5] = [(1, 1), (7, 13), (128, 128), (129, 128), (97, 211)];

/// The channel `plane` of the pixel `(red, green, blue)`, worked out here from the Rec. 709 weights (an oracle of its own).
fn channel(plane: Plane, [red, green, blue]: [f32; 3]) -> f32 {
    match plane {
        Plane::Red => red,
        Plane::Green => green,
        Plane::Blue => blue,
        _ => red * 0.2126 + green * 0.7152 + blue * 0.0722,
    }
}

#[test]
fn sum_min_and_max_of_each_channel_are_the_cpus() -> crate::common::Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    for (seed, &(height, width)) in SIZES.iter().enumerate() {
        let values = seeded(height, width, seed);
        let picture = Picture::new(height, width, &values)?;
        let on_device = block_on(gpu.upload(&picture))?;
        for plane in [Plane::Red, Plane::Green, Plane::Blue, Plane::Luma] {
            let channel: Vec<f32> = values
                .as_chunks::<3>()
                .0
                .iter()
                .map(|&pixel| self::channel(plane, pixel))
                .collect();
            let sum: f64 = channel.iter().map(|&value| f64::from(value)).sum();
            let (low, high) = channel
                .iter()
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), &value| {
                    (low.min(value), high.max(value))
                });
            let got = |how| block_on(gpu.reduce(&on_device, plane, how));
            let first_sum = got(Reduction::Sum)?;
            let device_sum = f64::from(first_sum);
            assert!(
                (device_sum - sum).abs() <= 1e-6 * sum.abs().max(1.0),
                "{plane:?} sum at {height} x {width}: {device_sum} on the device, {sum} in f64"
            );
            // a luma worked on the device may differ from this one by a rounding of its own; the extremes of a colour are exact
            let near = if plane == Plane::Luma { 1e-6 } else { 0.0 };
            assert!(
                (got(Reduction::Min)? - low).abs() <= near && (got(Reduction::Max)? - high).abs() <= near,
                "{plane:?} extremes at {height} x {width}: {} .. {} on the device, {low} .. {high}",
                got(Reduction::Min)?,
                got(Reduction::Max)?
            );
        }
    }
    Ok(())
}

#[test]
fn a_histogram_counts_the_bins_the_cpu_does_and_every_pixel_once() -> crate::common::Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    for (seed, &(height, width)) in SIZES.iter().enumerate() {
        let values = seeded(height, width, seed);
        let picture = Picture::new(height, width, &values)?;
        let on_device = block_on(gpu.upload(&picture))?;
        for bins in [1, 2, 7, 64, MOST_BINS] {
            for plane in [Plane::Red, Plane::Luma] {
                let mut counts = vec![0_u32; bins];
                for &pixel in values.as_chunks::<3>().0 {
                    // the bin the CPU's effects use: floor(value x bins), at most the last
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        clippy::cast_precision_loss,
                        reason = "a value of 0 to 1 times at most 256 bins"
                    )]
                    let bin = ((channel(plane, pixel) * bins as f32) as usize).min(bins - 1);
                    counts[bin] += 1;
                }
                let got = block_on(gpu.histogram(&on_device, plane, bins))?;
                // luma worked on the device may put a value on the other side of a bin edge by a rounding of its own: every
                // pixel is still counted once, and at most a few cross
                assert_eq!(
                    got.iter().sum::<u32>() as usize,
                    height * width,
                    "{plane:?} {bins} bins"
                );
                let moved: u32 = got.iter().zip(&counts).map(|(a, b)| a.abs_diff(*b)).sum();
                assert!(
                    moved <= if plane == Plane::Luma { 4 } else { 0 },
                    "{plane:?} {bins} bins at {height} x {width}: {moved} pixels counted in another bin"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn a_histogram_of_a_picture_beyond_65536_pixels_in_one_bin_counts_them_all() -> crate::common::Outcome {
    let Some(gpu) = gpu() else { return Ok(()) };
    // one flat grey: every pixel in one bin, more than the 16 bits a partial count keeps
    let (height, width) = (300, 300);
    let values = vec![0.35_f32; height * width * 3];
    let on_device = block_on(gpu.upload(&Picture::new(height, width, &values)?))?;
    let got = block_on(gpu.histogram(&on_device, Plane::Red, 10))?;
    assert_eq!((got[3], got.iter().sum::<u32>()), (90_000, 90_000), "{got:?}");
    assert!(block_on(gpu.histogram(&on_device, Plane::Red, 0)).is_err());
    assert!(block_on(gpu.histogram(&on_device, Plane::Red, MOST_BINS + 1)).is_err());
    Ok(())
}
