//! [`GpuImage`]: a picture on the device, and how it gets there and back.

use std::fmt;

use super::{Bindings, Gpu, Header, failed, picture_bytes};
use crate::{Error, Image, Picture};

/// The kernel that spreads an upload into one vec4 a pixel.
const UNPACK: (&str, &str) = ("unpack", include_str!("unpack.wgsl"));

/// A picture held on a [`Gpu`]: `height` rows of `width` pixels, red, green, blue and, if it has one, straight alpha, at the
/// levels an [`Image`] holds. How it is stored is the core's (today one `vec4<f32>` a pixel). It belongs to the `Gpu` that
/// made it: handing it to another is an error. Read it back with [`Gpu::download`].
pub struct GpuImage {
    height: usize,
    width: usize,
    alpha: bool,
    buffer: wgpu::Buffer,
    owner: Gpu,
}

impl fmt::Debug for GpuImage {
    /// The shape only.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GpuImage")
            .field("height", &self.height)
            .field("width", &self.width)
            .field("channels", &self.channels())
            .finish_non_exhaustive()
    }
}

impl GpuImage {
    /// The number of rows.
    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    /// The number of columns.
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// The number of channels of each pixel: 3 (red, green, blue), or 4 with alpha last, as for [`Image::channels`].
    #[must_use]
    pub const fn channels(&self) -> usize {
        if self.alpha { 4 } else { 3 }
    }

    /// The picture of `height` x `width` (alpha if `alpha`) in `buffer` on `owner`.
    pub(crate) const fn from_buffer(
        owner: Gpu,
        height: usize,
        width: usize,
        alpha: bool,
        buffer: wgpu::Buffer,
    ) -> Self {
        Self {
            height,
            width,
            alpha,
            buffer,
            owner,
        }
    }

    /// The same picture, sharing its storage: for a render that starts from it and writes elsewhere.
    pub(crate) fn share(&self) -> Self {
        Self::from_buffer(
            self.owner.clone(),
            self.height,
            self.width,
            self.alpha,
            self.buffer.clone(),
        )
    }

    /// Whether it has alpha.
    pub(crate) const fn has_alpha(&self) -> bool {
        self.alpha
    }

    /// The storage buffer that holds the picture: `height` x `width` pixels, each one `vec4<f32>` (red, green, blue and alpha, straight
    /// alpha, the levels an [`Image`] holds; alpha is 1 where the picture has none), row after row. For a library that
    /// works on the same wgpu device (the shared vocabulary of the packages) and reads the picture without a readback.
    /// Read it only: it is shared with whatever else holds the picture.
    #[must_use]
    pub const fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    /// This picture, if it is one of `gpu`'s.
    ///
    /// # Errors
    /// [`Error::Inputs`] if another context made it: the caller's programming error, not the machine's.
    pub(crate) fn of(&self, gpu: &Gpu) -> Result<&Self, Error> {
        if self.owner.same(gpu) {
            Ok(self)
        } else {
            Err(Error::Inputs(format!(
                "this GpuImage belongs to another Gpu context (on {}): a picture stays with the context that made it",
                self.owner.adapter()
            )))
        }
    }
}

impl Gpu {
    /// The picture of `height` x `width` pixels that `buffer` holds, as a [`GpuImage`] of this context: the inverse of
    /// [`GpuImage::buffer`]. `buffer` must be on this context's device, hold `vec4<f32>` a pixel (straight alpha; alpha is
    /// ignored and read as 1 if `alpha` is false) and have the `STORAGE`, `COPY_SRC` and `COPY_DST` usages. Nothing is
    /// uploaded, so nothing is counted.
    ///
    /// # Errors
    /// [`Error::Gpu`] for a size larger than the device holds; [`Error::Inputs`] for a buffer too small for it or lacking a usage.
    pub fn adopt(&self, buffer: wgpu::Buffer, height: usize, width: usize, alpha: bool) -> Result<GpuImage, Error> {
        self.fits(height, width)?;
        if buffer.size() < picture_bytes(height, width) {
            return Err(Error::Inputs(format!(
                "a buffer of {} bytes holds no picture of {height} x {width} pixels ({} bytes)",
                buffer.size(),
                picture_bytes(height, width)
            )));
        }
        let wanted = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST;
        let missing = wanted - buffer.usage();
        if !missing.is_empty() {
            return Err(Error::Inputs(format!(
                "the buffer lacks the usages {missing:?}: a picture's buffer needs STORAGE, COPY_SRC and COPY_DST"
            )));
        }
        Ok(GpuImage::from_buffer(self.clone(), height, width, alpha, buffer))
    }

    /// `picture` sent to the device: a [`GpuImage`] of its size without alpha. Counted as one upload.
    ///
    /// # Errors
    /// [`Error::Gpu`] for a picture larger than the device holds (naming both sizes) or a device that fails.
    pub async fn upload(&self, picture: &Picture<'_>) -> Result<GpuImage, Error> {
        self.upload_values(picture.height(), picture.width(), picture.values(), false)
            .await
    }

    /// `image` sent to the device: a [`GpuImage`] of its size, with its alpha if it has one. Counted as one upload.
    ///
    /// # Errors
    /// As [`upload`](Self::upload).
    pub async fn upload_image(&self, image: &Image) -> Result<GpuImage, Error> {
        match image.alpha() {
            None => self.upload(&image.as_picture()).await,
            Some(alpha) => {
                let values: Vec<f32> = image
                    .as_picture()
                    .pixels()
                    .iter()
                    .zip(alpha)
                    .flat_map(|(&[red, green, blue], &alpha)| [red, green, blue, alpha])
                    .collect();
                self.upload_values(image.height(), image.width(), &values, true).await
            }
        }
    }

    async fn upload_values(&self, height: usize, width: usize, values: &[f32], alpha: bool) -> Result<GpuImage, Error> {
        self.fits(height, width)?;
        let channels = if alpha { 4.0 } else { 3.0 };
        let buffer = self
            .guarded("take a picture", || {
                let sent = self.send(values, true);
                let buffer = self.picture_buffer(height, width);
                let mut encoder = self.encoder();
                self.dispatch(
                    &mut encoder,
                    UNPACK,
                    Header::plain(height, width),
                    &[[channels, 0.0, 0.0, 0.0]],
                    &Bindings {
                        source: None,
                        dest: &buffer,
                        data: Some(&sent),
                        other: None,
                    },
                );
                self.queue().submit([encoder.finish()]);
                buffer
            })
            .await?;
        Ok(GpuImage::from_buffer(self.clone(), height, width, alpha, buffer))
    }

    /// `image` read back from the device: the [`Image`] it holds, with alpha only if it has alpha. Counted as one download.
    ///
    /// # Errors
    /// [`Error::Inputs`] if another context made `image`; [`Error::Gpu`] if the device fails (is lost) while reading it.
    pub async fn download(&self, image: &GpuImage) -> Result<Image, Error> {
        let image = image.of(self)?;
        let values = self.read(image.buffer(), image.height(), image.width()).await?;
        let quads = values.as_chunks::<4>().0;
        let pixels = quads.iter().map(|&[red, green, blue, _]| [red, green, blue]).collect();
        let made = Image::from_pixels(image.height(), image.width(), pixels);
        Ok(if image.alpha {
            made.with_alpha(quads.iter().map(|quad| quad[3]).collect())
        } else {
            made
        })
    }

    /// The picture of `height` x `width` in `buffer` read back, four floats a pixel.
    pub(super) async fn read(&self, buffer: &wgpu::Buffer, height: usize, width: usize) -> Result<Vec<f32>, Error> {
        let size = picture_bytes(height, width);
        let staging = self
            .guarded("read a picture back", || {
                let staging = self.device().create_buffer(&wgpu::BufferDescriptor {
                    label: Some("bullterrier-image readback"),
                    size,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let mut encoder = self.encoder();
                encoder.copy_buffer_to_buffer(buffer, 0, &staging, 0, size);
                self.queue().submit([encoder.finish()]);
                staging
            })
            .await?;
        let slice = staging.slice(..);
        let (send, receive) = futures_channel::oneshot::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            // the receiver waits below; if it has gone, so has the caller
            drop(send.send(result));
        });
        self.wait()?;
        receive
            .await
            .map_err(|_| Error::Gpu("the GPU dropped a readback (the device was lost)".to_owned()))?
            .map_err(|error| failed("read a picture back", error))?;
        let view = slice
            .get_mapped_range()
            .map_err(|error| failed("read a picture back", error))?;
        let values = view
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&bytes| f32::from_le_bytes(bytes))
            .collect();
        drop(view);
        staging.unmap();
        self.count(false, size);
        Ok(values)
    }
}
