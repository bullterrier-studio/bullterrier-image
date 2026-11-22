//! The GPU backend (feature `gpu`): [`Gpu`], the core's context on a wgpu device, and [`GpuImage`], a picture held on it.
//!
//! The CPU is the reference: a render on the GPU is the same recipe held to a bound, not to the byte (see
//! `docs/design-decisions.md`, "GPU backend"). A render on the GPU is a call on the context ([`Gpu::render`]), the one
//! place that chooses it; every GPU call is `async`, with no blocking twin. Effects never touch a device, they declare a
//! [`Kernel`] that the one executor runs. A picture lives on the device as `vec4<f32>` per pixel in a storage buffer (straight alpha, the levels an
//! [`Image`](crate::Image) holds) and is read with the CPU's own maths, never through hardware filtering.

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use crate::Error;

mod image;
mod kernel;

pub use image::GpuImage;
pub use kernel::GpuKernel;
pub(crate) use kernel::{Bindings, Block, Encoded, Header, Kernel};

/// The WGSL every kernel is compiled with: the binding layout, index helpers, the sRGB transfer and the luma weights.
const PRELUDE: &str = include_str!("gpu/prelude.wgsl");

/// The side of a workgroup: 16 x 16 invocations, one a pixel.
const GROUP: u32 = 16; // constant: the `@workgroup_size(16, 16)` of every kernel (prelude.wgsl)

/// Bytes of a pixel on the device: four f32.
const PIXEL_BYTES: u64 = 16; // constant: a vec4<f32>

/// The storage buffers a kernel binds (source, destination, data, other): the device must allow as many per stage.
const STORAGE_BINDINGS: u32 = 4; // constant: the binding layout of prelude.wgsl

/// A GPU context: a wgpu device and queue with the core's kernels compiled on it, as they are first needed, and a count
/// of the pictures that crossed between the CPU and the device. Cloning it is cheap and shares all of that; it can be
/// shared between threads. Two contexts are equal only if one is a clone of the other.
///
/// Render on it with [`render`](Self::render), which keeps the picture on the device, and read it back with
/// [`download`](Self::download). The core never makes a device of its own except in [`request`](Self::request).
#[derive(Clone)]
pub struct Gpu {
    context: Arc<Context>,
}

struct Context {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter: String,
    /// The most pixels a picture on this device holds.
    largest: usize,
    /// The longest side a kernel dispatch covers: workgroups per dimension times their side.
    longest_side: usize,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: Mutex<HashMap<&'static str, wgpu::ComputePipeline>>,
    /// What a kernel's unused storage bindings hold.
    empty: wgpu::Buffer,
    uploads: AtomicU64,
    downloads: AtomicU64,
    bytes: AtomicU64,
}

/// What has crossed between the CPU and a [`Gpu`] since it was made or last reset: pictures and maps sent to the device
/// and pictures read back. A kernel's parameters are not counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Transfers {
    /// Pictures and maps sent to the device.
    pub uploads: u64,
    /// Pictures read back from it.
    pub downloads: u64,
    /// The bytes of both.
    pub bytes: u64,
}

impl fmt::Debug for Gpu {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Gpu")
            .field("adapter", &self.context.adapter)
            .field("largest", &self.context.largest)
            .finish_non_exhaustive()
    }
}

impl PartialEq for Gpu {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.context, &other.context)
    }
}

impl Eq for Gpu {}

/// The bytes a picture of `height` x `width` takes on the device: the one place that knows a pixel's size there.
pub(crate) fn picture_bytes(height: usize, width: usize) -> u64 {
    (height * width) as u64 * PIXEL_BYTES
}

/// The failure of a wgpu call, as the core reports it.
pub(crate) fn failed(what: &str, error: impl fmt::Display) -> Error {
    Error::Gpu(format!("the GPU failed to {what}: {error}"))
}

/// The adapter a standalone context runs on: a high-performance one of `instance`.
async fn adapter_of(instance: &wgpu::Instance) -> Result<wgpu::Adapter, Error> {
    instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        })
        .await
        .map_err(|error| {
            Error::Gpu(format!(
                "no GPU adapter was found ({error}): this machine has no GPU the core can use (Metal, Vulkan or Direct3D 12); render on the CPU"
            ))
        })
}

/// Whether the GPU backend supports operating system `os` (`std::env::consts::OS`): macOS (Metal). The browser (WebGPU) is
/// supported too, but has no OS name: [`require_supported_platform`] skips this check there. `test_bed` is the one
/// exception, for CI only: Linux on Mesa's software Vulkan (`BT_GPU=llvmpipe`) is the only free GPU runner, so the GPU
/// tests run there.
fn supported(os: &str, test_bed: bool) -> Result<(), Error> {
    if os == "macos" || (os == "linux" && test_bed) {
        return Ok(());
    }
    Err(Error::Gpu(format!(
        "the GPU backend supports macOS and the browser; this is {os}: render on the CPU"
    )))
}

/// The one platform rule, asked first by every way to make a context, before any adapter or device work.
fn require_supported_platform() -> Result<(), Error> {
    if cfg!(target_arch = "wasm32") {
        return Ok(());
    }
    supported(
        std::env::consts::OS,
        std::env::var("BT_GPU").is_ok_and(|gpu| gpu == "llvmpipe"),
    )
}

impl Gpu {
    /// A context on a device of its own: the machine's high-performance adapter, with its own limits on the size of a
    /// storage buffer (so the largest picture is what the adapter holds, not WebGPU's default of 128 MiB).
    ///
    /// # Errors
    /// [`Error::Gpu`] on a platform other than macOS and the browser (the message names it), if there is no adapter, or it
    /// gives no device.
    pub async fn request() -> Result<Self, Error> {
        require_supported_platform()?;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = adapter_of(&instance).await?;
        let wanted = adapter.limits();
        let required_limits = wgpu::Limits {
            max_storage_buffer_binding_size: wanted.max_storage_buffer_binding_size,
            max_buffer_size: wanted.max_buffer_size,
            ..wgpu::Limits::downlevel_defaults()
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("bullterrier-image"),
                required_limits,
                ..Default::default()
            })
            .await
            .map_err(|error| failed("give a device", error))?;
        Self::new(device, queue)
    }

    /// A context on the caller's `device` and `queue`: the core compiles its kernels on that device and keeps its pictures
    /// there, so a caller's own GPU work and the core's share them. The caller keeps the device; the core sets no handler on
    /// it.
    ///
    /// # Errors
    /// [`Error::Gpu`] on a platform other than macOS and the browser (the message names it), or naming the limit, if the device allows fewer storage buffers a shader stage, bindings a group or
    /// invocations a workgroup than the core's kernels use.
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Result<Self, Error> {
        require_supported_platform()?;
        let limits = device.limits();
        let invocations = GROUP * GROUP;
        for (name, has, needs) in [
            (
                "max_storage_buffers_per_shader_stage",
                limits.max_storage_buffers_per_shader_stage,
                STORAGE_BINDINGS,
            ),
            (
                "max_bindings_per_bind_group",
                limits.max_bindings_per_bind_group,
                STORAGE_BINDINGS + 1,
            ),
            (
                "max_compute_invocations_per_workgroup",
                limits.max_compute_invocations_per_workgroup,
                invocations,
            ),
            (
                "max_compute_workgroup_size_x",
                limits.max_compute_workgroup_size_x,
                GROUP,
            ),
            (
                "max_compute_workgroup_size_y",
                limits.max_compute_workgroup_size_y,
                GROUP,
            ),
        ] {
            if has < needs {
                return Err(Error::Gpu(format!(
                    "the device's `{name}` is {has}; the GPU backend needs at least {needs}"
                )));
            }
        }
        let info = device.adapter_info();
        let adapter = format!(
            "{} ({:?}, {:?}, {})",
            info.name, info.backend, info.device_type, info.driver
        );
        let bytes = limits.max_storage_buffer_binding_size.min(limits.max_buffer_size);
        let largest = usize::try_from(bytes / PIXEL_BYTES).unwrap_or(usize::MAX);
        let longest_side = limits.max_compute_workgroups_per_dimension as usize * GROUP as usize;
        let entry = |binding: u32, ty: wgpu::BufferBindingType| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let read = wgpu::BufferBindingType::Storage { read_only: true };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bullterrier-image kernel"),
            entries: &[
                entry(0, wgpu::BufferBindingType::Uniform),
                entry(1, read),
                entry(2, wgpu::BufferBindingType::Storage { read_only: false }),
                entry(3, read),
                entry(4, read),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bullterrier-image kernel"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let empty = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bullterrier-image unused binding"),
            size: PIXEL_BYTES,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        Ok(Self {
            context: Arc::new(Context {
                device,
                queue,
                adapter,
                largest,
                longest_side,
                layout,
                pipeline_layout,
                pipelines: Mutex::new(HashMap::new()),
                empty,
                uploads: AtomicU64::new(0),
                downloads: AtomicU64::new(0),
                bytes: AtomicU64::new(0),
            }),
        })
    }

    /// The adapter the device is on, as its driver names it: name, backend, kind and driver
    /// (`"Apple M4 (Metal, IntegratedGpu, )"`, `"llvmpipe (LLVM 19.1.1, 256 bits) (Vulkan, Cpu, llvmpipe)"`).
    #[must_use]
    pub fn adapter(&self) -> &str {
        &self.context.adapter
    }

    /// The most pixels a picture on this device can have: the device's largest storage buffer over 16 bytes a pixel.
    #[must_use]
    pub fn largest_picture(&self) -> usize {
        self.context.largest
    }

    /// What has crossed between the CPU and this context since it was made or last [reset](Self::reset_transfers).
    #[must_use]
    pub fn transfers(&self) -> Transfers {
        Transfers {
            uploads: self.context.uploads.load(Ordering::Relaxed),
            downloads: self.context.downloads.load(Ordering::Relaxed),
            bytes: self.context.bytes.load(Ordering::Relaxed),
        }
    }

    /// Sets the count of [`transfers`](Self::transfers) back to zero.
    pub fn reset_transfers(&self) {
        self.context.uploads.store(0, Ordering::Relaxed);
        self.context.downloads.store(0, Ordering::Relaxed);
        self.context.bytes.store(0, Ordering::Relaxed);
    }

    fn count(&self, upload: bool, bytes: u64) {
        let counter = if upload {
            &self.context.uploads
        } else {
            &self.context.downloads
        };
        counter.fetch_add(1, Ordering::Relaxed);
        self.context.bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub(crate) fn device(&self) -> &wgpu::Device {
        &self.context.device
    }

    pub(crate) fn queue(&self) -> &wgpu::Queue {
        &self.context.queue
    }

    /// `Ok` if a picture of `height` x `width` fits on this device.
    ///
    /// # Errors
    /// [`Error::Gpu`] naming both sizes if it does not.
    pub(crate) fn fits(&self, height: usize, width: usize) -> Result<(), Error> {
        let pixels = height.saturating_mul(width);
        let side = height.max(width);
        if pixels > self.context.largest || side > self.context.longest_side {
            return Err(Error::Gpu(format!(
                "a picture of {height} x {width} ({pixels} pixels) is larger than the GPU holds: at most {} pixels and {} \
                 a side on {}; render it on the CPU",
                self.context.largest, self.context.longest_side, self.context.adapter
            )));
        }
        Ok(())
    }

    /// A buffer for a picture of `height` x `width` on the device, a kernel's destination and a copy's source.
    pub(crate) fn picture_buffer(&self, height: usize, width: usize) -> wgpu::Buffer {
        self.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("bullterrier-image picture"),
            size: picture_bytes(height, width).max(PIXEL_BYTES),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// `values` sent to the device in a storage buffer a kernel reads as its `data`; a picture or a map is `counted` as an
    /// upload, a kernel's table is not.
    pub(crate) fn send(&self, values: &[f32], counted: bool) -> wgpu::Buffer {
        let bytes: Vec<u8> = values.iter().flat_map(|value| value.to_le_bytes()).collect();
        let buffer = self.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("bullterrier-image data"),
            size: (bytes.len() as u64).max(PIXEL_BYTES),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue().write_buffer(&buffer, 0, &bytes);
        if counted {
            self.count(true, bytes.len() as u64);
        }
        buffer
    }

    /// The compute pipeline of the kernel `name` (whose WGSL, after the prelude, is `wgsl`), compiled the first time.
    fn pipeline(&self, name: &'static str, wgsl: &str) -> wgpu::ComputePipeline {
        let mut pipelines = self.context.pipelines.lock().unwrap_or_else(PoisonError::into_inner);
        pipelines
            .entry(name)
            .or_insert_with(|| {
                let module = self.device().create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(name),
                    source: wgpu::ShaderSource::Wgsl(format!("{PRELUDE}\n{wgsl}").into()),
                });
                self.device().create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(name),
                    layout: Some(&self.context.pipeline_layout),
                    module: &module,
                    entry_point: Some("main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    cache: None,
                })
            })
            .clone()
    }

    /// Records into `encoder` one run of the kernel `name` (`wgsl` after the prelude) over a picture of `header`'s size,
    /// with its parameter `block` and `bindings`.
    pub(crate) fn dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        (name, wgsl): (&'static str, &str),
        header: Header,
        block: &[[f32; 4]],
        bindings: &Bindings<'_>,
    ) {
        let pipeline = self.pipeline(name, wgsl);
        let uniform = header.with_block(block);
        let settings = self.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some(name),
            size: uniform.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue().write_buffer(&settings, 0, &uniform);
        let empty = &self.context.empty;
        let group = self.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(name),
            layout: &self.context.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: settings.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: bindings.source.unwrap_or(empty).as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: bindings.dest.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: bindings.data.unwrap_or(empty).as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: bindings.other.unwrap_or(empty).as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(name),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(header.width.div_ceil(GROUP), header.height.div_ceil(GROUP), 1);
    }

    /// A new command encoder.
    pub(crate) fn encoder(&self) -> wgpu::CommandEncoder {
        self.device().create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bullterrier-image"),
        })
    }

    /// `work` done with the device's validation and out-of-memory errors caught, so a failure is an [`Error::Gpu`] naming
    /// `what` instead of wgpu's panic: its value once the errors are in.
    ///
    /// # Errors
    /// [`Error::Gpu`] for the first error the device reports.
    pub(crate) async fn guarded<T>(&self, what: &str, work: impl FnOnce() -> T) -> Result<T, Error> {
        let memory = self.device().push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = self.device().push_error_scope(wgpu::ErrorFilter::Validation);
        let value = work();
        // popped before anything is awaited: a scope's guard stays on its thread
        let (validation, memory) = (validation.pop(), memory.pop());
        if let Some(error) = validation.await.or(memory.await) {
            return Err(failed(what, error));
        }
        Ok(value)
    }

    /// Blocks until the device has finished its submitted work (native); on the web the work is awaited instead.
    pub(crate) fn wait(&self) -> Result<(), Error> {
        #[cfg(not(target_arch = "wasm32"))]
        self.device()
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| failed("finish its work", error))?;
        Ok(())
    }

    /// Whether `other` is this context.
    pub(crate) fn same(&self, other: &Self) -> bool {
        self == other
    }
}

#[cfg(test)]
mod tests {
    //! The refusal of a machine with no adapter, where it is raised: no public route reaches an instance with no backend,
    //! and a machine with a GPU has an adapter.

    use std::pin::pin;
    use std::task::{Context, Poll, Waker};

    use super::{adapter_of, supported};
    use crate::{Error, ErrorKind};

    fn refusal(os: &str, test_bed: bool) -> Option<String> {
        match supported(os, test_bed) {
            Ok(()) => None,
            Err(Error::Gpu(message)) => Some(message),
            Err(other) => panic!("a platform is refused as Error::Gpu, not {other:?}"),
        }
    }

    #[test]
    fn macos_is_supported_and_other_systems_are_refused_naming_themselves() {
        assert_eq!(refusal("macos", false), None);
        for os in ["linux", "windows", "freebsd"] {
            let message = refusal(os, false).unwrap_or_else(|| panic!("{os} is refused"));
            assert!(message.contains(&format!("this is {os}")), "{message}");
            assert!(message.contains("macOS and the browser"), "{message}");
        }
    }

    #[test]
    fn the_test_bed_allows_linux_only() {
        assert_eq!(refusal("linux", true), None);
        assert!(refusal("windows", true).is_some());
        assert!(refusal("freebsd", true).is_some());
    }

    #[test]
    fn an_instance_with_no_adapter_is_refused_naming_why() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::empty(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        // with no backend there is nothing to wait for: the answer is there at the first poll
        let polled = pin!(adapter_of(&instance)).poll(&mut Context::from_waker(Waker::noop()));
        let Poll::Ready(refused) = polled else {
            unreachable!("an instance with no backend answers at once")
        };
        let refused = refused.err();
        assert!(
            refused
                .as_ref()
                .is_some_and(|error| error.kind() == ErrorKind::Gpu && error.to_string().contains("no GPU adapter")),
            "{refused:?}"
        );
    }
}
