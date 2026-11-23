//! What a kernel run is given: the uniform header every kernel reads, then its own block, and the buffers it binds.

use crate::effect::Check;
use crate::{Image, Inputs, Place};

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

    /// Where the picture lies on its canvas.
    pub(crate) const fn place(&self) -> Place {
        Place {
            canvas_height: self.place[0] as usize,
            canvas_width: self.place[1] as usize,
            first_row: self.place[2] as usize,
        }
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

impl Encoded {
    /// A kernel's `block` and the `table` of numbers it reads, if any.
    pub(crate) const fn new(block: Block, table: Option<Vec<f32>>) -> Self {
        Self { block, table }
    }
}

/// What a planning kernel is given besides its parameters and the picture's header: the step's `Inputs` (the tables and
/// pictures it names, its place and size ratio, as the CPU's `apply` is given them) and, for a kernel that
/// [reads the picture](Kernel::reading_picture), the picture itself, read back first (one counted download).
pub(crate) struct Given<'a> {
    pub(crate) inputs: &'a Inputs<'a>,
    pub(crate) picture: Option<&'a Image>,
}

/// How a kernel turns a step's parameters into what it is given.
enum How<P> {
    /// From the parameters and the header alone: the block and, if it reads one, the table.
    Pure {
        block: fn(&P, &Header) -> Block,
        table: Option<fn(&P, &Header) -> Vec<f32>>,
    },
    /// With what only the CPU's own code can make (see [`Given`]).
    Planned(fn(&P, &Header, &Given<'_>) -> Option<Encoded>),
}

impl<P> Clone for How<P> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<P> Copy for How<P> {}

/// What a file in `src/effects/` declares to run its effect on the GPU: `pub(crate) fn kernel() -> Kernel<Params>`, beside
/// its `<effect>.wgsl` (the prelude's bindings and helpers, its `struct Block` and `fn main`).
pub(crate) struct Kernel<P> {
    wgsl: &'static str,
    how: How<P>,
    only: Option<fn(&P) -> bool>,
    sees: Option<fn(&P) -> bool>,
    bound: f32,
}

// by hand: a derive would ask `P: Copy`, and the kernel holds only pointers to functions of `P`
impl<P> Clone for Kernel<P> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<P> Copy for Kernel<P> {}

/// The bound of every kernel today: the largest difference from the CPU's f32 output, on a value of 0 to 1.
const DEFAULT_BOUND: f32 = 1e-4; // constant: the GPU decision's bound for a per-pixel effect (docs/design-decisions.md)

impl<P> Kernel<P> {
    /// The kernel whose WGSL is `wgsl` and whose uniform block `block` makes from the effect's parameters, held to the
    /// default bound, 1e-4.
    pub(crate) const fn new(wgsl: &'static str, block: fn(&P, &Header) -> Block) -> Self {
        Self {
            wgsl,
            how: How::Pure { block, table: None },
            only: None,
            sees: None,
            bound: DEFAULT_BOUND,
        }
    }

    /// The kernel whose WGSL is `wgsl` and whose block and table `plan` makes from the parameters, the header and what only
    /// the CPU can make: the step's `Inputs`, and the picture if the kernel [reads it](Self::reading_picture). The plan calls
    /// the CPU's own functions, so the definition stays in one place. A plan of a kernel that reads the picture may answer
    /// `None` for a picture it cannot hold to the bound (the stretch of a nearly flat one): the step runs on the CPU, counted.
    pub(crate) const fn planned(wgsl: &'static str, plan: fn(&P, &Header, &Given<'_>) -> Option<Encoded>) -> Self {
        Self {
            wgsl,
            how: How::Planned(plan),
            only: None,
            sees: None,
            bound: DEFAULT_BOUND,
        }
    }

    /// This kernel's plan reads the picture for the parameters `when` accepts (a palette found in it, quantiles of it): the
    /// executor submits the work before it and reads the picture back, counted, to give it as `Given::picture`.
    pub(crate) const fn reading_picture(mut self, when: fn(&P) -> bool) -> Self {
        self.sees = Some(when);
        self
    }

    /// This kernel held to `bound` instead of the default, which the effect's file says why beside the call: the largest
    /// difference from the CPU's output, on a value of 0 to 1 (at most an eighth of an 8-bit level, which the parity test
    /// holds every bound to).
    pub(crate) const fn with_bound(mut self, bound: f32) -> Self {
        self.bound = bound;
        self
    }

    /// This kernel reading `table` as its `data` (a curve's pieces, a list of marks the CPU's own generator drew), worked out
    /// from the parameters and the picture's header.
    pub(crate) const fn with_table(mut self, table: fn(&P, &Header) -> Vec<f32>) -> Self {
        if let How::Pure { table: slot, .. } = &mut self.how {
            *slot = Some(table);
        }
        self
    }

    /// This kernel running only for the parameters `only` accepts; a step with any other runs on the CPU (counted), as one
    /// of an effect with no kernel does. For a setting the kernel does not do (a blur, a serial scan): the kernel's
    /// parity test holds the rest, and the defaults must be accepted.
    pub(crate) const fn only_where(mut self, only: fn(&P) -> bool) -> Self {
        self.only = Some(only);
        self
    }
}

/// How a step of the effect called `name` turns its parameters into what its kernel is given.
type Encode =
    Box<dyn Fn(&serde_json::Value, &Header, &Given<'_>) -> Result<Option<Encoded>, crate::Error> + Send + Sync>;

/// Whether the kernel runs the parameters of a step.
type Runs = Box<dyn Fn(&serde_json::Value) -> bool + Send + Sync>;

/// The GPU kernel of one of the core's own effects (see [`Effect::kernel`](crate::Effect::kernel)): what the executor of a
/// GPU render runs for its steps, held to a stated bound of the CPU's output. Only the core makes them; an effect that
/// lives elsewhere has none and runs on the CPU inside a GPU render.
pub struct GpuKernel {
    name: &'static str,
    wgsl: &'static str,
    bound: f32,
    encode: Encode,
    runs: Runs,
    reads: Runs,
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
    pub(crate) fn of<P: serde::de::DeserializeOwned + 'static>(
        name: &'static str,
        kernel: Kernel<P>,
        check: Option<Check<P>>,
    ) -> Self {
        let Kernel {
            wgsl,
            how,
            only,
            sees,
            bound,
        } = kernel;
        Self {
            name,
            wgsl,
            bound,
            encode: Box::new(move |params, header, given| {
                let params = P::deserialize(params).map_err(|source| crate::Error::Params { effect: name, source })?;
                // the effect's own refusal, the one `apply` makes, before anything is built
                if let Some(check) = check {
                    check(&params, given.inputs)?;
                }
                Ok(match how {
                    How::Pure { block, table } => Some(Encoded::new(
                        block(&params, header),
                        table.map(|table| table(&params, header)),
                    )),
                    How::Planned(plan) => plan(&params, header, given),
                })
            }),
            // parameters that do not read back are not the kernel's to judge: the CPU run reports them
            runs: Box::new(move |params| {
                only.is_none_or(|only| P::deserialize(params).is_ok_and(|params| only(&params)))
            }),
            reads: Box::new(move |params| {
                sees.is_some_and(|sees| P::deserialize(params).is_ok_and(|params| sees(&params)))
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

    /// What the kernel is given for a step run with `params` (complete and scaled, as `Effect::apply` takes them); none if its
    /// plan leaves the step to the CPU.
    ///
    /// # Errors
    /// [`Error::Params`] for parameters that do not read back, else what the effect's `check` refuses.
    pub(crate) fn encode(
        &self,
        params: &serde_json::Value,
        header: &Header,
        given: &Given<'_>,
    ) -> Result<Option<Encoded>, crate::Error> {
        (self.encode)(params, header, given)
    }

    /// Whether the kernel's plan reads the picture for a step with `params` (see [`Kernel::reading_picture`]).
    pub(crate) fn reads_picture(&self, params: &serde_json::Value) -> bool {
        (self.reads)(params)
    }

    /// Whether the kernel runs a step with `params`: not one with a setting it leaves to the CPU.
    pub(crate) fn runs(&self, params: &serde_json::Value) -> bool {
        (self.runs)(params)
    }
}
