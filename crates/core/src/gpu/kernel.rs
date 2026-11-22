//! What a kernel run is given: the uniform header every kernel reads, then its own block, and the buffers it binds.

use crate::Place;

/// The first part of every kernel's uniform (`Header` in prelude.wgsl): the picture's size, where it lies on its canvas and
/// working pixels per design pixel.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Header {
    pub(crate) width: u32,
    pub(crate) height: u32,
    place: [u32; 3],
    ratio: f32,
}

impl Header {
    /// The header of a picture of `height` x `width` that lies at `place` on its canvas, at `ratio` working pixels per design
    /// pixel. The sizes fit in `u32`: a picture on the device is at most its longest side (`Gpu::fits`).
    pub(crate) fn new(height: usize, width: usize, place: Place, ratio: f64) -> Self {
        let narrow = |value: usize| u32::try_from(value).unwrap_or(u32::MAX);
        Self {
            width: narrow(width),
            height: narrow(height),
            place: [
                narrow(place.canvas_height),
                narrow(place.canvas_width),
                narrow(place.first_row),
            ],
            ratio: crate::ops::cast::single(ratio),
        }
    }

    /// The header of a picture of `height` x `width` that is its own canvas, at one working pixel per design pixel: for a
    /// kernel that reads neither.
    pub(crate) fn plain(height: usize, width: usize) -> Self {
        Self::new(height, width, Place::new(height, width, 0), 1.0)
    }

    /// The bytes of the uniform: this header (32 bytes) and then `block`, little-endian as WGSL reads them.
    pub(crate) fn with_block(self, block: &[[f32; 4]]) -> Vec<u8> {
        let words = [
            self.width,
            self.height,
            self.place[0],
            self.place[1],
            self.place[2],
            self.ratio.to_bits(),
            0,
            0,
        ];
        let mut bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
        // an empty block is one vec4 of zeros: WGSL has no empty struct
        let block: &[[f32; 4]] = if block.is_empty() { &[[0.0; 4]] } else { block };
        bytes.extend(block.iter().flatten().flat_map(|value| value.to_le_bytes()));
        bytes
    }
}

/// The buffers one kernel run binds (prelude.wgsl): the picture it reads (`source`), the one it writes (`dest`), the floats
/// it reads (`data`: a map, a table) and a second picture (`other`). An unused binding holds an empty buffer.
pub(crate) struct Bindings<'a> {
    pub(crate) source: Option<&'a wgpu::Buffer>,
    pub(crate) dest: &'a wgpu::Buffer,
    pub(crate) data: Option<&'a wgpu::Buffer>,
    pub(crate) other: Option<&'a wgpu::Buffer>,
}

/// A kernel's uniform block: its parameters as vec4s.
pub(crate) type Block = Vec<[f32; 4]>;

/// The parameters a kernel's uniform block holds (vec4s, so the layout is the same in Rust and WGSL with no padding rule to
/// keep), and the floats of its `data` binding, if it reads any (a table).
pub(crate) struct Encoded {
    pub(crate) block: Block,
    pub(crate) table: Option<Vec<f32>>,
}

/// What a file in `src/effects/` declares to run its effect on the GPU: `pub(crate) fn kernel() -> Kernel<Params>`, beside
/// its `<effect>.wgsl` (the prelude's bindings and helpers, its `struct Block` and `fn main`).
pub(crate) struct Kernel<P> {
    wgsl: &'static str,
    block: fn(&P) -> Block,
    table: Option<fn(&P) -> Vec<f32>>,
    bound: f32,
}

// by hand: a derive would ask `P: Copy`, and the kernel holds only pointers to functions of `P`
impl<P> Clone for Kernel<P> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<P> Copy for Kernel<P> {}

/// The bound of every kernel today: the largest difference from the CPU's f32 output, on a value of 0 to 1. A kernel that
/// needs a looser one gets a `with_bound` and says why beside it.
const DEFAULT_BOUND: f32 = 1e-4; // constant: the GPU decision's bound for a per-pixel effect (docs/design-decisions.md)

impl<P> Kernel<P> {
    /// The kernel whose WGSL is `wgsl` and whose uniform block `block` makes from the effect's parameters, held to the
    /// default bound, 1e-4.
    pub(crate) const fn new(wgsl: &'static str, block: fn(&P) -> Block) -> Self {
        Self {
            wgsl,
            block,
            table: None,
            bound: DEFAULT_BOUND,
        }
    }

    /// This kernel reading `table` of the parameters as its `data` (a curve's pieces).
    pub(crate) const fn with_table(mut self, table: fn(&P) -> Vec<f32>) -> Self {
        self.table = Some(table);
        self
    }
}

/// How a step of the effect called `name` turns its parameters into what its kernel is given.
type Encode = Box<dyn Fn(&serde_json::Value) -> Result<Encoded, crate::Error> + Send + Sync>;

/// The GPU kernel of one of the core's own effects (see [`Effect::kernel`](crate::Effect::kernel)): what the executor of a
/// GPU render runs for its steps, held to a stated bound of the CPU's output. Only the core makes them; an effect that
/// lives elsewhere has none and runs on the CPU inside a GPU render.
pub struct GpuKernel {
    name: &'static str,
    wgsl: &'static str,
    bound: f32,
    encode: Encode,
}

impl std::fmt::Debug for GpuKernel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GpuKernel")
            .field("name", &self.name)
            .field("bound", &self.bound)
            .finish_non_exhaustive()
    }
}

impl GpuKernel {
    /// The kernel `kernel` of the effect called `name`, whose parameters are read as `P`.
    pub(crate) fn of<P: serde::de::DeserializeOwned + 'static>(name: &'static str, kernel: Kernel<P>) -> Self {
        let Kernel {
            wgsl,
            block,
            table,
            bound,
        } = kernel;
        Self {
            name,
            wgsl,
            bound,
            encode: Box::new(move |params| {
                let params = P::deserialize(params).map_err(|source| crate::Error::Params { effect: name, source })?;
                Ok(Encoded {
                    block: block(&params),
                    table: table.map(|table| table(&params)),
                })
            }),
        }
    }

    /// The largest difference this kernel's output may have from the CPU's, value by value, on a value of 0 to 1 (before any
    /// rounding to 8 bits): its parity tests hold it to that.
    #[must_use]
    pub const fn bound(&self) -> f32 {
        self.bound
    }

    /// The name and WGSL the executor compiles it by.
    pub(crate) const fn source(&self) -> (&'static str, &'static str) {
        (self.name, self.wgsl)
    }

    /// What the kernel is given for a step run with `params` (complete and scaled, as `Effect::apply` takes them).
    pub(crate) fn encode(&self, params: &serde_json::Value) -> Result<Encoded, crate::Error> {
        (self.encode)(params)
    }
}
