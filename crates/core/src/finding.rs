//! Finding a map of a picture with a model: the catalogue of models the package knows ([`models`]), how a photo is
//! made into the tensor a model takes, and how its answer becomes a map ([`ModelFinder`]). Running the model is
//! [`Infer`]'s: the one part that needs a runtime.

#[cfg(feature = "onnx")]
pub mod download;
#[cfg(feature = "onnx")]
pub mod onnx;

use crate::ops::length::{float, whole};
use crate::ops::resample::{Filter, Resize};
use crate::{Error, Finder, Layer, Picture};

/// What a model is for, which fixes what it is given and what its answer means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ModelKind {
    /// A salient-object model (IS-Net): the photo as a 1024 x 1024 square, answering the probability that each pixel
    /// belongs to the subject.
    Subject,
    /// A monocular depth model (Depth Anything V2): the photo with its short side 518 px and both sides multiples of 14
    /// (a long strip is scaled down until its long side is at most 1092 px, which bounds the memory), answering relative
    /// inverse depth, larger nearer, in no fixed unit.
    Depth,
}

/// A model the package knows how to run: where it comes from, and the checksum its file must have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Model {
    /// The model's name, which is also the name of its file in the cache folder, with `.onnx`.
    pub name: &'static str,
    /// Where the file is downloaded from.
    pub url: &'static str,
    /// The SHA-256 of the file, as 64 lowercase hex digits: a file with another is refused, not used.
    pub sha256: &'static str,
    /// The size of the file in bytes.
    pub size: u64,
    /// What the model finds.
    pub kind: ModelKind,
}

/// IS-Net (general use), the salient-object model of Qin et al. (ECCV 2022, Apache-2.0), in the ONNX export that the rembg
/// project publishes (MIT).
pub const IS_NET: Model = Model {
    name: "isnet-general-use",
    url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/isnet-general-use.onnx",
    sha256: "60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a",
    size: 178_648_008,
    kind: ModelKind::Subject,
};

/// Depth Anything V2 Small (Yang et al., `NeurIPS` 2024, Apache-2.0), in the fp32 ONNX export of the `onnx-community`, pinned
/// to a commit.
pub const DEPTH_ANYTHING_V2_SMALL: Model = Model {
    name: "depth-anything-v2-small",
    url: "https://huggingface.co/onnx-community/depth-anything-v2-small/resolve/f7421df0cc30f121782ab050d42f0a423291dcde/onnx/model.onnx",
    sha256: "afb6a5c28f3b6bf1618c6e43f02073ef9dfdc70e937502d51603e57b0a1df10c",
    size: 99_060_839,
    kind: ModelKind::Depth,
};

/// The models the package knows.
#[must_use]
pub fn models() -> &'static [Model] {
    &[IS_NET, DEPTH_ANYTHING_V2_SMALL]
}

/// A tensor of `f32`: the shape of a model's input or answer and its values, row-major.
#[derive(Clone, Debug, PartialEq)]
pub struct Tensor {
    shape: Vec<usize>,
    values: Vec<f32>,
}

impl Tensor {
    /// The tensor of `shape` holding `values`.
    ///
    /// # Errors
    /// [`Error::BackendUnavailable`] if the shape's product is not the number of values (a model answered with something
    /// of another size than it says).
    pub fn new(shape: Vec<usize>, values: Vec<f32>) -> Result<Self, Error> {
        let wanted: usize = shape.iter().product();
        if wanted != values.len() {
            return Err(Error::BackendUnavailable(format!(
                "a tensor of shape {shape:?} holds {wanted} values, not {}",
                values.len()
            )));
        }
        Ok(Self { shape, values })
    }

    /// The shape, outermost first.
    #[must_use]
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// The values, row-major.
    #[must_use]
    pub fn values(&self) -> &[f32] {
        &self.values
    }
}

/// Runs a model: its first output for one input. Implemented by the ONNX Runtime backend; a test implements it with a
/// stand-in, the one external boundary of finding.
pub trait Infer: Send + Sync {
    /// The model's first output for `input`.
    ///
    /// # Errors
    /// [`Error::BackendUnavailable`] if the model cannot run here or cannot take `input`.
    fn infer(&self, input: &Tensor) -> Result<Tensor, Error>;
}

/// The strength of the edge clean-up of a subject: pulled in by 1.5 px, blurred by 1 px, for a 1000-1600 px photo.
const SHRINK: f64 = 1.5; // constant: the default `shrink` of `subject_matte`, tuned by eye on 1000-1600 px photos
const FEATHER: f64 = 1.0; // constant: the default `feather` of `subject_matte`
/// A pixel is the subject's when the model is at least this sure: the even split of a probability.
const SURE: f32 = 0.5; // constant: half, the even split of a probability

/// The square a subject model takes, px.
const SUBJECT_SIDE: usize = 1024; // constant: the input size IS-Net was trained at
/// The short side a depth model takes, px, and the patch its sides are a whole number of.
const DEPTH_SHORT_SIDE: f64 = 518.0; // constant: Depth Anything's trained input size (37 patches of 14 px)
const DEPTH_PATCH: f64 = 14.0; // constant: the ViT-S/14 encoder's patch size
/// The longest side a depth model is given, px: the model's attention grows with the square of the number of 14 px patches,
/// so a long strip is scaled down to fit. Measured on the model (ONNX Runtime, CPU), peak memory with a 518 px short side:
/// 0.55 GB at 686, 1.3 GB at 1400, 2.7 GB at 2254; with IS-Net loaded too (2.9 GB at 686) a long side of 1092 (78 patches)
/// keeps both under 3.3 GB, well under the 4 GB a process may take.
const DEPTH_MAX_LONG_SIDE: f64 = 1092.0; // constant: 78 patches of 14 px; the largest long side whose measured peak leaves headroom
/// `ImageNet`'s mean and std per channel (`torchvision`), which Depth Anything was trained with.
const DEPTH_MEAN: [f32; 3] = [0.485, 0.456, 0.406]; // constant: ImageNet's per-channel mean
const DEPTH_STD: [f32; 3] = [0.229, 0.224, 0.225]; // constant: ImageNet's per-channel std
/// What is subtracted from a subject model's input, in the 0..1 range of a picture.
const SUBJECT_MEAN: f32 = 0.5; // constant: IS-Net's normalisation, v - 0.5 on 0..1

/// A [`Finder`] that asks a model: the photo is made into the model's input, the answer into a map of the photo's size.
///
/// A subject comes back with its edge cleaned ([`refine_edge`](crate::refine_edge), `shrink` 1.5 px and `feather` 1 px)
/// and as [`Error::MapNotFound`] of kind subject when no pixel is at least half sure. A depth comes back scaled to
/// 0..1, nearest 1 and farthest 0 (all 0 when the model tells nothing apart).
pub struct ModelFinder<R> {
    kind: ModelKind,
    runner: R,
    shrink: f64,
    feather: f64,
}

impl<R: Infer> ModelFinder<R> {
    /// A finder for a model of `kind`, run by `runner`.
    #[must_use]
    pub const fn new(kind: ModelKind, runner: R) -> Self {
        Self {
            kind,
            runner,
            shrink: SHRINK,
            feather: FEATHER,
        }
    }

    /// The model's own answer for a depth finder: the relative inverse depth of `picture` (larger is nearer) stretched to the
    /// photo's size, row after row, not scaled to the photo: the same value in two photos is the same nearness, so photos can
    /// be scaled alike. Only the order and the ratios of differences mean anything: it is neither a distance nor bounded.
    ///
    /// # Errors
    /// [`Error::BackendUnavailable`] if this is not a depth finder, or the model cannot run or answers with the wrong size.
    pub fn inverse_depth(&self, picture: &Picture<'_>) -> Result<Tensor, Error> {
        if self.kind != ModelKind::Depth {
            return Err(Error::BackendUnavailable(
                "only a depth model answers an inverse depth".to_owned(),
            ));
        }
        let (input, side) = depth_input(picture);
        let answer = self.answer(&input, side)?;
        Tensor::new(
            vec![picture.height(), picture.width()],
            inverse_depth(&answer.values, side, (picture.height(), picture.width())),
        )
    }

    /// This finder with the edge clean-up of a subject set to `shrink` and `feather` px ([`refine_edge`](crate::refine_edge)):
    /// the defaults are 1.5 and 1, for a photo of 1000 to 1600 px. A depth is not cleaned.
    #[must_use]
    pub const fn with_edge(mut self, shrink: f64, feather: f64) -> Self {
        self.shrink = shrink;
        self.feather = feather;
        self
    }
}

impl<R: Infer> ModelFinder<R> {
    /// The model's answer for `input`, checked to be the `side` (rows, columns) it was asked about.
    fn answer(&self, input: &Tensor, side: (usize, usize)) -> Result<Tensor, Error> {
        let answer = self.runner.infer(input)?;
        let (rows, columns) = side;
        if answer.values.len() != rows * columns {
            return Err(Error::BackendUnavailable(format!(
                "the model answered with {} values, not the {} of a {rows} x {columns} map: it is not the model it should be",
                answer.values.len(),
                rows * columns
            )));
        }
        Ok(answer)
    }
}

impl<R: Infer> Finder for ModelFinder<R> {
    fn find(&self, picture: &Picture<'_>) -> Result<Layer, Error> {
        let (height, width) = (picture.height(), picture.width());
        let (input, side) = match self.kind {
            ModelKind::Subject => (subject_input(picture), (SUBJECT_SIDE, SUBJECT_SIDE)),
            ModelKind::Depth => depth_input(picture),
        };
        let answer = self.answer(&input, side)?;
        match self.kind {
            ModelKind::Subject => subject_map(&answer.values, side, (height, width), (self.shrink, self.feather)),
            ModelKind::Depth => depth_map(&answer.values, side, (height, width)),
        }
    }
}

/// `picture` resized to `to` (rows, columns) by `filter`, as planar channels (`3 x rows x columns`), each value `v`
/// as `(v - mean) / std`.
fn planes(picture: &Picture<'_>, to: (usize, usize), filter: Filter, mean: [f32; 3], std: [f32; 3]) -> Tensor {
    let pixels = picture.values().as_chunks::<3>().0;
    let resized = Resize::to_size(picture.height(), picture.width(), to, filter).apply(pixels);
    let area = to.0 * to.1;
    let mut values = vec![0.0; 3 * area];
    for (index, pixel) in resized.iter().enumerate() {
        for channel in 0..3 {
            values[channel * area + index] = (pixel[channel] - mean[channel]) / std[channel];
        }
    }
    Tensor {
        shape: vec![1, 3, to.0, to.1],
        values,
    }
}

fn subject_input(picture: &Picture<'_>) -> Tensor {
    planes(
        picture,
        (SUBJECT_SIDE, SUBJECT_SIDE),
        Filter::Bilinear,
        [SUBJECT_MEAN; 3],
        [1.0; 3],
    )
}

/// The tensor a depth model takes for `picture` and its size: scaled so the short side is 518 px, or, for a long strip,
/// so the long side is 1092 px, then each side to the nearest multiple of 14 (at least one patch).
fn depth_input(picture: &Picture<'_>) -> (Tensor, (usize, usize)) {
    let (short, long) = (
        picture.height().min(picture.width()),
        picture.height().max(picture.width()),
    );
    let scale = (DEPTH_SHORT_SIDE / float(short)).min(DEPTH_MAX_LONG_SIDE / float(long));
    let side = |length: usize| whole(float(length) * scale / DEPTH_PATCH).max(1) * whole(DEPTH_PATCH);
    let to = (side(picture.height()), side(picture.width()));
    (planes(picture, to, Filter::Bicubic, DEPTH_MEAN, DEPTH_STD), to)
}

/// `values` (`from` rows x columns) resized to `to`, bilinear, as a map.
fn stretched(values: &[f32], from: (usize, usize), to: (usize, usize)) -> Vec<f32> {
    if from == to {
        return values.to_vec();
    }
    let pixels: Vec<[f32; 1]> = values.iter().map(|&value| [value]).collect();
    Resize::to_size(from.0, from.1, to, Filter::Bilinear)
        .apply(&pixels)
        .into_iter()
        .map(|[value]| value)
        .collect()
}

/// The matte of a photo of `to` (rows, columns) from a subject model's answer: `probability` (0..1, row after row) at the
/// model's own size `from`. It is stretched to the photo, its edge cleaned ([`refine_edge`](crate::refine_edge), `shrink`
/// `shrink` and `feather` px; 1.5 and 1 are the defaults) and, if no pixel is at least half sure, the picture has no
/// subject. This is what
/// [`ModelFinder`] does with a [`ModelKind::Subject`] answer, for a backend that is not a [`ModelFinder`].
///
/// # Errors
/// [`Error::MapNotFound`] of kind subject if no value is at least 0.5; [`Error::Inputs`] if `probability` is not
/// `from.0 * from.1` values.
pub fn subject_matte(
    probability: &[f32],
    from: (usize, usize),
    to: (usize, usize),
    (shrink, feather): (f64, f64),
) -> Result<Layer, Error> {
    if probability.len() != from.0 * from.1 {
        return Err(Error::Inputs(format!(
            "{} values are not a map of {} x {} pixels",
            probability.len(),
            from.0,
            from.1
        )));
    }
    subject_map(probability, from, to, (shrink, feather))
}

fn subject_map(
    answer: &[f32],
    from: (usize, usize),
    to: (usize, usize),
    (shrink, feather): (f64, f64),
) -> Result<Layer, Error> {
    let probability: Vec<f32> = answer.iter().map(|value| value.clamp(0.0, 1.0)).collect();
    if !probability.iter().any(|&value| value >= SURE) {
        return Err(Error::map_not_found(
            "subject",
            "no subject found in the picture (no person, pet or object stood out)",
        ));
    }
    let matte = model_map("subject", to, stretched(&probability, from, to))?;
    Ok(crate::refine_edge(&matte, shrink, feather))
}

/// The depth map of a photo of `to` from a depth model's `answer` at its own size `from`: the inverse depth stretched to the
/// photo ([`inverse_depth`]) scaled to 0..1, nearest 1 and farthest 0 (all 0 when the model tells nothing apart).
fn depth_map(answer: &[f32], from: (usize, usize), to: (usize, usize)) -> Result<Layer, Error> {
    let raw = inverse_depth(answer, from, to);
    let (low, span) = spread(&raw);
    let values = if span == 0.0 {
        vec![0.0; raw.len()]
    } else {
        raw.iter().map(|&value| (value - low) / span).collect()
    };
    // a value that is not a number made the spread, and so every value, not a number: the map refuses it here, where
    // every map is checked
    model_map("depth", to, values)
}

/// `answer` (`from` rows x columns) stretched bilinear to `to`, on the model's own scale: the stretch keeps 0..1, so the
/// answer is scaled to 0..1 first (an affine rescale commutes with a convex mix) and back after.
fn inverse_depth(answer: &[f32], from: (usize, usize), to: (usize, usize)) -> Vec<f32> {
    let (low, span) = spread(answer);
    if span == 0.0 {
        return vec![low; to.0 * to.1];
    }
    let scaled: Vec<f32> = answer.iter().map(|&value| (value - low) / span).collect();
    stretched(&scaled, from, to)
        .into_iter()
        .map(|value| value * span + low)
        .collect()
}

/// The map called `name` that a model's `values` make for a photo of `size` (rows, columns): the one place a model's answer
/// is held to the rule of a map (finite, within 0..1), and refused as that map's own error when it breaks it.
fn model_map(name: &str, size: (usize, usize), values: Vec<f32>) -> Result<Layer, Error> {
    Layer::new(size.0, size.1, values)
        .map_err(|error| Error::map_not_found(name, format!("the {name} model's answer is no map: {error}")))
}

/// The smallest of `values` and the distance to the largest. The order is the total one, in which a NaN is the largest
/// (or the smallest) of all, so a NaN makes the spread a NaN instead of being skipped: scaling by it gives a NaN map.
fn spread(values: &[f32]) -> (f32, f32) {
    let low = values.iter().copied().min_by(f32::total_cmp).unwrap_or(0.0);
    let high = values.iter().copied().max_by(f32::total_cmp).unwrap_or(0.0);
    (low, high - low)
}
