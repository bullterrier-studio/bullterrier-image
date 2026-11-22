//! Reductions of a picture on the device: a sum, a minimum, a maximum or a histogram of one channel. The tree is fixed (the same
//! workgroup shape and the same order every time), so one backend repeats its answer; a histogram counts whole numbers, so
//! it is exact. Only the answer, a few numbers, comes back: the picture stays where it is.

use super::{Bindings, Gpu, GpuImage, Header};
use crate::Error;

/// The channel of a picture that a reduction reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Plane {
    /// Red.
    Red,
    /// Green.
    Green,
    /// Blue.
    Blue,
    /// The Rec. 709 luma of red, green and blue, as every effect that reads brightness takes it.
    Luma,
}

/// What a reduction makes of the values of a channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Reduction {
    /// Their sum, added in f32 in a fixed tree.
    Sum,
    /// The smallest.
    Min,
    /// The largest.
    Max,
}

/// Pixels a workgroup folds (reduce.wgsl and histogram.wgsl: 256 threads of 64 items each).
const CHUNK: usize = 256 * 64; // constant: the workgroup shape of reduce.wgsl and histogram.wgsl

/// The most bins a histogram has (one thread a bin in histogram.wgsl).
pub const MOST_BINS: usize = 256; // constant: a workgroup of 256 threads, one bin each

const REDUCE: (&str, &str) = ("reduce", include_str!("reduce.wgsl"));
const HISTOGRAM: (&str, &str) = ("histogram", include_str!("histogram.wgsl"));
const HISTOGRAM_TOTAL: (&str, &str) = ("histogram_total", include_str!("histogram_total.wgsl"));

/// Four whole numbers as a uniform block (the bits of u32s: the shader reads them back as u32).
fn words(first: usize, second: usize, third: usize) -> [f32; 4] {
    let word = |value: usize| f32::from_bits(u32::try_from(value).unwrap_or(u32::MAX));
    [word(first), word(second), word(third), 0.0]
}

impl Plane {
    const fn index(self) -> usize {
        match self {
            Self::Red => 0,
            Self::Green => 1,
            Self::Blue => 2,
            Self::Luma => 3,
        }
    }
}

impl Gpu {
    /// The sum, minimum or maximum of one channel of `image`, worked on the device; one number comes back (counted as a
    /// download). A minimum or maximum is exact; a sum is added in f32 in a fixed tree, so a picture gives the same sum every
    /// time on one device, within about 1e-6 of the true sum relative to its size.
    ///
    /// # Errors
    /// [`Error::Inputs`] if another context made `image`; [`Error::Gpu`] if the device fails.
    pub async fn reduce(&self, image: &GpuImage, plane: Plane, how: Reduction) -> Result<f32, Error> {
        let image = image.of(self)?;
        let pixels = image.height() * image.width();
        let groups = pixels.div_ceil(CHUNK);
        let operation = match how {
            Reduction::Sum => 0,
            Reduction::Min => 1,
            Reduction::Max => 2,
        };
        let (partial, total) = self
            .guarded("reduce a picture", || {
                let (partial, total) = (self.picture_buffer(1, groups), self.picture_buffer(1, 1));
                let mut encoder = self.encoder();
                self.dispatch_groups(
                    &mut encoder,
                    REDUCE,
                    Header::plain(1, pixels),
                    &[words(plane.index(), operation, pixels)],
                    &Bindings {
                        source: Some(image.buffer()),
                        dest: &partial,
                        data: None,
                        other: None,
                    },
                    groups,
                );
                // the partial results, red only, reduced by one workgroup
                self.dispatch_groups(
                    &mut encoder,
                    REDUCE,
                    Header::plain(1, groups),
                    &[words(0, operation, groups)],
                    &Bindings {
                        source: Some(&partial),
                        dest: &total,
                        data: None,
                        other: None,
                    },
                    1,
                );
                self.queue().submit([encoder.finish()]);
                (partial, total)
            })
            .await?;
        let answer = self.read(&total, 1, 1).await?;
        drop(partial);
        Ok(answer[0])
    }

    /// How many pixels of `image` fall in each of `bins` bins of one channel: bin `floor(value x bins)`, at most `bins - 1` (a
    /// negative value is in the first), as the CPU's effects bin a brightness. Counted on the device, exact; `bins` numbers
    /// come back (counted as a download).
    ///
    /// # Errors
    /// [`Error::Recipe`] unless `bins` is 1 to [`MOST_BINS`]; [`Error::Inputs`] if another context made `image`;
    /// [`Error::Gpu`] if the device fails.
    pub async fn histogram(&self, image: &GpuImage, plane: Plane, bins: usize) -> Result<Vec<u32>, Error> {
        let image = image.of(self)?;
        if bins == 0 || bins > MOST_BINS {
            return Err(Error::Recipe(format!(
                "a histogram has 1 to {MOST_BINS} bins, not {bins}"
            )));
        }
        let pixels = image.height() * image.width();
        let groups = pixels.div_ceil(CHUNK);
        let (partial, total) = self
            .guarded("count a picture", || {
                let (partial, total) = (self.picture_buffer(1, groups * bins), self.picture_buffer(1, bins));
                let mut encoder = self.encoder();
                self.dispatch_groups(
                    &mut encoder,
                    HISTOGRAM,
                    Header::plain(1, pixels),
                    &[words(plane.index(), bins, pixels)],
                    &Bindings {
                        source: Some(image.buffer()),
                        dest: &partial,
                        data: None,
                        other: None,
                    },
                    groups,
                );
                self.dispatch_groups(
                    &mut encoder,
                    HISTOGRAM_TOTAL,
                    Header::plain(1, bins),
                    &[words(bins, groups, 0)],
                    &Bindings {
                        source: Some(&partial),
                        dest: &total,
                        data: None,
                        other: None,
                    },
                    1,
                );
                self.queue().submit([encoder.finish()]);
                (partial, total)
            })
            .await?;
        let read = self.read(&total, 1, bins).await?;
        drop(partial);
        // two exact floats a bin: the high and the low 16 bits of its count
        Ok(read
            .as_chunks::<4>()
            .0
            .iter()
            .map(|quad| to_whole(quad[0]) << 16 | to_whole(quad[1]))
            .collect())
    }
}

/// The whole number a float of an exact count holds (0 to 65535).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a count of at most 16 bits, exact in an f32"
)]
const fn to_whole(value: f32) -> u32 {
    value as u32
}
