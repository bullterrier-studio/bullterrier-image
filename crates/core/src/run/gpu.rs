//! The GPU executor: a recipe run with its picture on a [`Gpu`]. The run begins on the CPU as any render does (the recipe
//! checked, the photo framed and resized, the maps found, every step prepared); then the working picture is sent once and
//! the steps run in order: a run of steps that have kernels is recorded into one command encoder and submitted together,
//! and a step without one runs on the CPU, exactly as [`Prepared::apply`] runs it, between a readback and an upload.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::assets::Assets;
use super::cut::Cut;
use super::regions::Evaluator;
use super::size::{Origin, ratio_for};
use super::step::{GpuStep, Prepared, prepare};
use super::{Photo, Run};
use crate::gpu::{Bindings, Encoded, Header};
use crate::ops::cast::single;
use crate::ops::par;
use crate::{BlendMode, Error, Frame, Gpu, GpuImage, Image, Layer, Picture, Presets, Recipe};

/// The kernel that lays a step's result over its input (`ops/blend.rs` `lay`).
const LAY: (&str, &str) = ("lay", include_str!("../gpu/lay.wgsl"));

/// The kernel that gives the finished picture its alpha (a `frame.transparent` cut).
const ALPHA: (&str, &str) = ("alpha", include_str!("../gpu/alpha.wgsl"));

/// The refusal of a recipe in a `render_from`, saying what it needs and why it cannot have it yet.
fn refuse_from(what: &str) -> Error {
    Error::Gpu(format!(
        "`render_from` renders a picture already on the GPU, which has no {what}: render the photo with `render` for it"
    ))
}

impl Gpu {
    /// The picture `recipe` makes of `photo`, kept on this device: [`Photo::render`] on this GPU without the readback, for
    /// a caller whose next work is on the device too. `options` are as for a render (their GPU, if they name one, is this
    /// one). A `frame.transparent` cut is made on the device, so the picture has alpha.
    ///
    /// # Errors
    /// As [`Photo::render`] with [`Options::with_gpu`](crate::Options::with_gpu), and [`Error::Gpu`] if `options` name
    /// another GPU.
    pub async fn render(
        &self,
        photo: &Photo<'_>,
        recipe: &Recipe,
        presets: &Presets,
        options: crate::Options,
    ) -> Result<GpuImage, Error> {
        self.own(&options)?;
        let (image, cut) = photo_on(self, photo, recipe, presets, &options).await?;
        match cut {
            None => Ok(image),
            Some(cut) => self.cut(&image, &cut).await,
        }
    }

    /// [`render`](Self::render), blocking until the picture is made.
    ///
    /// # Errors
    /// As [`render`](Self::render).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn render_blocking(
        &self,
        photo: &Photo<'_>,
        recipe: &Recipe,
        presets: &Presets,
        options: crate::Options,
    ) -> Result<GpuImage, Error> {
        pollster::block_on(self.render(photo, recipe, presets, options))
    }

    /// The picture `recipe` makes of `image`, a picture already on this device (a [`render`](Self::render)'s result, the
    /// next look of a chain): nothing is sent or read back but for a step that runs on the CPU. `image` is the working
    /// picture as it is, so its size is the render's: a `size` (of the options or the recipe) other than its longest side,
    /// and a `frame` other than the default, need a resample and are refused for now. It has no maps, so a recipe that uses
    /// one (a `mask`, a field, a region) is refused too. Its alpha is not read: effects never see it.
    ///
    /// # Errors
    /// [`Error::Gpu`] for `image` of another context, a recipe that needs a resample or a map, or `options` naming another
    /// GPU or one a GPU render does not take; else as [`render`](Self::render).
    pub async fn render_from(
        &self,
        image: &GpuImage,
        recipe: &Recipe,
        presets: &Presets,
        options: crate::Options,
    ) -> Result<GpuImage, Error> {
        let image = image.of(self)?;
        self.own(&options)?;
        options.gpu_takes()?;
        let uses = recipe.check_rules(presets)?;
        if let Some(reached) = uses.reach(&recipe.regions).first() {
            return Err(refuse_from(&format!("maps (the recipe uses `{}`)", reached.name)));
        }
        let (height, width) = (image.height(), image.width());
        if recipe.frame != Frame::default() {
            return Err(refuse_from("`frame` (a frame resamples the picture)"));
        }
        if let Some(size) = options.size().or(recipe.size)
            && usize::try_from(size).ok() != Some(height.max(width))
        {
            return Err(refuse_from(&format!(
                "other size: `size` {size} asks for a resample of a picture of {height} x {width}"
            )));
        }
        let ratio = ratio_for(height, width, &options)?;
        let (no_luts, no_pictures) = (BTreeMap::new(), BTreeMap::new());
        let mut assets = Assets::new(&no_luts, &no_pictures);
        let steps = recipe
            .steps
            .iter()
            .enumerate()
            .map(|(index, entry)| prepare(presets, ratio, &mut assets, entry, &format!("step {}", index + 1)))
            .collect::<Result<Vec<_>, _>>()?;
        let mut regions = Evaluator::new(&recipe.regions, BTreeMap::new(), BTreeMap::new(), ratio, None);
        let origin = Origin::whole(height, width);
        steps_on(
            self,
            steps,
            (ratio, origin),
            options.threads(),
            &mut regions,
            None,
            image.share(),
        )
        .await
    }

    /// [`render_from`](Self::render_from), blocking until the picture is made.
    ///
    /// # Errors
    /// As [`render_from`](Self::render_from).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn render_from_blocking(
        &self,
        image: &GpuImage,
        recipe: &Recipe,
        presets: &Presets,
        options: crate::Options,
    ) -> Result<GpuImage, Error> {
        pollster::block_on(self.render_from(image, recipe, presets, options))
    }

    /// `Ok` unless `options` name a GPU other than this one.
    fn own(&self, options: &crate::Options) -> Result<(), Error> {
        match options.gpu() {
            Some(other) if other != self => Err(Error::Gpu(format!(
                "the options name another Gpu ({}) than the one rendering ({})",
                other.adapter(),
                self.adapter()
            ))),
            _ => Ok(()),
        }
    }

    /// `image` given the alpha of `cut`, on the device.
    async fn cut(&self, image: &GpuImage, cut: &Cut) -> Result<GpuImage, Error> {
        let (height, width) = (image.height(), image.width());
        self.guarded("cut the picture out", || {
            let alpha = self.send(&cut.alpha(height, width), true);
            let dest = self.picture_buffer(pixel_bytes(height, width));
            let mut encoder = self.encoder();
            self.dispatch(
                &mut encoder,
                ALPHA,
                Header::plain(height, width),
                &[],
                &Bindings {
                    source: Some(image.buffer()),
                    dest: &dest,
                    data: Some(&alpha),
                    other: None,
                },
            );
            self.queue().submit([encoder.finish()]);
            GpuImage::from_buffer(self.clone(), height, width, true, dest)
        })
        .await
    }
}

/// The bytes of a picture of `height` x `width` on the device.
fn pixel_bytes(height: usize, width: usize) -> u64 {
    (height * width) as u64 * 16 // constant: a vec4<f32> a pixel
}

/// [`Photo::render`] on `gpu`: the picture read back and cut out on the CPU.
///
/// # Errors
/// As [`Photo::render`].
pub(super) async fn render(
    gpu: &Gpu,
    photo: &Photo<'_>,
    recipe: &Recipe,
    presets: &Presets,
    options: &crate::Options,
) -> Result<Image, Error> {
    let (image, cut) = photo_on(gpu, photo, recipe, presets, options).await?;
    let image = gpu.download(&image).await?;
    Ok(match cut {
        Some(cut) => cut.apply(image),
        None => image,
    })
}

/// The steps of `recipe` run on `photo` on `gpu`, and the cut still to make of the result.
async fn photo_on(
    gpu: &Gpu,
    photo: &Photo<'_>,
    recipe: &Recipe,
    presets: &Presets,
    options: &crate::Options,
) -> Result<(GpuImage, Option<Cut>), Error> {
    options.gpu_takes()?;
    let Run {
        working,
        origin,
        ratio,
        steps,
        cut,
        mut regions,
        ..
    } = par::install(options.threads(), || photo.start(recipe, presets, options))??;
    let picture = working.picture();
    gpu.fits(picture.height(), picture.width())?;
    let start = gpu.upload(&picture)?;
    let threads = options.threads();
    let image = steps_on(
        gpu,
        steps.collect(),
        (ratio, origin),
        threads,
        &mut regions,
        Some(picture),
        start,
    )
    .await?;
    Ok((image, cut))
}

/// `steps` run on `start`, the working picture on `gpu` (`working` on the CPU, if it is there: the regions are made from
/// it), each step as its kernel where it has one, else on the CPU, on `threads` threads, between a readback and an upload.
///
/// # Errors
/// [`Error::Recipe`] for no steps; why a region could not be made, an effect failed, or the device failed.
async fn steps_on(
    gpu: &Gpu,
    steps: Vec<Prepared>,
    (ratio, origin): (f64, Origin),
    threads: usize,
    regions: &mut Evaluator,
    working: Option<Picture<'_>>,
    start: GpuImage,
) -> Result<GpuImage, Error> {
    if steps.is_empty() {
        return Err(Error::Recipe("a recipe needs at least one step".to_owned()));
    }
    let (height, width) = (start.height(), start.width());
    let header = Header::new(height, width, crate::Place::new(height, width, 0), ratio);
    let plan: Vec<Option<GpuStep<'_>>> = steps.iter().map(Prepared::on_gpu).collect();
    let mut current = start;
    let mut index = 0;
    while index < steps.len() {
        let end = (index..steps.len())
            .find(|&at| plan[at].is_none())
            .unwrap_or(steps.len());
        if end > index {
            // a run of kernels: one encoder, one submission
            let first = current;
            current = gpu
                .guarded("run a step", || {
                    let mut encoder = gpu.encoder();
                    let mut current = first;
                    for (at, step) in plan
                        .iter()
                        .enumerate()
                        .take(end)
                        .skip(index)
                        .filter_map(|(at, step)| Some((at, step.as_ref()?)))
                    {
                        let mask = match &step.overlay.mask {
                            Some(name) => {
                                let working = working.ok_or_else(|| refuse_from("maps"))?;
                                Some(regions.selection(name, &working)?)
                            }
                            None => None,
                        };
                        current = kernel_step(gpu, &mut encoder, step, mask, &current, header)?;
                        regions.release(at);
                    }
                    gpu.queue().submit([encoder.finish()]);
                    Ok::<_, Error>(current)
                })
                .await??;
            index = end;
        } else {
            // a step on the CPU, run as a render without the GPU runs it
            let before = gpu.download(&current).await?;
            let picture = before.as_picture();
            let after = par::install(threads, || {
                steps[index].apply(&picture, ratio, regions, &working.unwrap_or(picture), (0, origin))
            })??;
            current = gpu.upload_image(&after)?;
            regions.release(index);
            index += 1;
        }
    }
    Ok(current)
}

/// Records `step` run on `current` into `encoder`, and its result laid over `current` by its blend mode, opacity and
/// `mask` (none: the whole of it) unless it is plain; the picture it makes.
fn kernel_step(
    gpu: &Gpu,
    encoder: &mut wgpu::CommandEncoder,
    step: &GpuStep<'_>,
    mask: Option<Arc<Layer>>,
    current: &GpuImage,
    header: Header,
) -> Result<GpuImage, Error> {
    let (height, width) = (current.height(), current.width());
    let Encoded { block, table } = step.kernel.encode(step.params)?;
    let table = table.map(|table| gpu.send(&table, false));
    let after = gpu.picture_buffer(pixel_bytes(height, width));
    gpu.dispatch(
        encoder,
        step.kernel.source(),
        header,
        &block,
        &Bindings {
            source: Some(current.buffer()),
            dest: &after,
            data: table.as_ref(),
            other: None,
        },
    );
    if step.overlay.is_plain() {
        return Ok(GpuImage::from_buffer(gpu.clone(), height, width, false, after));
    }
    let mask = mask.map(|mask| gpu.send(mask.values(), true));
    let laid = gpu.picture_buffer(pixel_bytes(height, width));
    let mode = match step.overlay.blend_mode {
        BlendMode::Normal => 0.0,
        BlendMode::Multiply => 1.0,
        BlendMode::Screen => 2.0,
        BlendMode::Overlay => 3.0,
        BlendMode::SoftLight => 4.0,
        BlendMode::Difference => 5.0,
        BlendMode::Color => 6.0,
    };
    let masked = if mask.is_some() { 1.0 } else { 0.0 };
    gpu.dispatch(
        encoder,
        LAY,
        header,
        &[[mode, single(step.overlay.opacity), masked, 0.0]],
        &Bindings {
            source: Some(current.buffer()),
            dest: &laid,
            data: mask.as_ref(),
            other: Some(&after),
        },
    );
    Ok(GpuImage::from_buffer(gpu.clone(), height, width, false, laid))
}
