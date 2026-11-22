//! The GPU backend (feature `gpu`), at small sizes. `BT_GPU` says what GPU the machine has: unset, the tests need one
//! and fail without it; `none`, each GPU test returns early and passes (Rust has no skip at run time), writing its name
//! to the file `BT_GPU_SKIPS` names, which `poe rust-test-gpu` counts aloud (`tools/gpu_skips.py`); any other value is
//! part of the adapter's name the tests must find (CI sets `llvmpipe`, Mesa's software Vulkan, with `BT_GPU_EXPERIMENTAL=1`, which
//! lets the backend run on Linux).

mod dither;
mod gradient_map;
mod halftone;
mod lut;
mod parity;
mod recipes;
mod reduce;
mod render;
mod tables;
mod transfer;
mod wide;

use std::cell::{Cell, RefCell};
use std::ops::Deref;
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

use bullterrier_image::{Error, Gpu, Image, Options, Photo, Presets, Recipe};
use pollster::block_on;

/// The context the tests share, or none where `BT_GPU=none` says the machine has no GPU.
static SHARED: LazyLock<Option<Gpu>> = LazyLock::new(fresh_context);

/// One GPU test at a time. The Vulkan loader (Ubuntu's `libvulkan1`, under lavapipe in CI) crashed with SIGSEGV in
/// `vkSetDebugUtilsObjectNameEXT`, naming a buffer of one context, while another test's new context enumerated its
/// devices (4 of 10 CI runs, 2026-11-22; wgpu names objects in debug builds): contexts of separate instances used at once
/// from several threads. The library holds no state between contexts; this is the loader's race, kept out of the tests.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

thread_local! {
    /// How many contexts this thread's test holds, and the lock while it holds any: a test may hold two.
    static HELD: Cell<usize> = const { Cell::new(0) };
    static GUARD: RefCell<Option<MutexGuard<'static, ()>>> = const { RefCell::new(None) };
}

/// This thread's share of the lock: taken by the first, let go with the last.
struct Alone;

impl Alone {
    fn take() -> Self {
        if HELD.get() == 0 {
            let guard = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
            GUARD.with_borrow_mut(|held| *held = Some(guard));
        }
        HELD.set(HELD.get() + 1);
        Self
    }
}

impl Drop for Alone {
    fn drop(&mut self) {
        HELD.set(HELD.get() - 1);
        if HELD.get() == 0 {
            GUARD.with_borrow_mut(Option::take);
        }
    }
}

/// A context a test holds: no other GPU test runs until it is dropped (with the test's other contexts).
pub(crate) struct Held {
    gpu: Gpu,
    _alone: Alone,
}

impl Deref for Held {
    type Target = Gpu;
    fn deref(&self) -> &Gpu {
        &self.gpu
    }
}

/// The context `make` gives, held, the lock taken before it is made; none (the lock let go) where it gives none.
fn held(make: impl FnOnce() -> Option<Gpu>) -> Option<Held> {
    let alone = Alone::take();
    make().map(|gpu| Held { gpu, _alone: alone })
}

/// The shared context, or none (after noting the skip) where the machine has no GPU.
pub(crate) fn gpu() -> Option<Held> {
    let shared = held(|| SHARED.clone());
    if shared.is_none() {
        skipped();
    }
    shared
}

/// Notes that the running test skips for want of a GPU: its name (libtest names its thread so), one line in the file
/// `BT_GPU_SKIPS` names, if it names one.
///
/// # Panics
/// If that file cannot be written: a skip nobody counts is what this guards against.
fn skipped() {
    let Ok(path) = std::env::var("BT_GPU_SKIPS") else {
        return;
    };
    let test = std::thread::current().name().unwrap_or("a GPU test").to_owned();
    let written = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, format!("{test}\n").as_bytes()));
    assert!(
        written.is_ok(),
        "{test}: the skip could not be written to {path}: {written:?}"
    );
}

/// A context of its own, held, for a test that counts transfers; none where the machine has no GPU.
pub(crate) fn fresh() -> Option<Held> {
    held(fresh_context)
}

/// A new context; none where the machine has no GPU. Made only under the lock ([`held`]).
///
/// # Panics
/// If there is no adapter and `BT_GPU` does not say `none`, or the adapter is not the one `BT_GPU` names.
#[expect(clippy::print_stderr, reason = "a skipped test says why, as BT_GPU=none asks")]
#[expect(clippy::panic, reason = "a missing GPU fails the tests that need it, with its error")]
fn fresh_context() -> Option<Gpu> {
    let expected = std::env::var("BT_GPU").unwrap_or_default();
    if expected == "none" {
        eprintln!("skipped: BT_GPU=none says this machine has no GPU");
        skipped();
        return None;
    }
    let gpu =
        block_on(Gpu::request()).unwrap_or_else(|error| panic!("{error} (set BT_GPU=none on a machine without a GPU)"));
    assert!(
        gpu.adapter().to_lowercase().contains(&expected.to_lowercase()),
        "the adapter is {}, not the {expected:?} BT_GPU names",
        gpu.adapter()
    );
    Some(gpu)
}

/// `recipe` rendered on `photo` on `gpu` and read back, blocking: what a native caller writes.
pub(crate) fn rendered(gpu: &Gpu, photo: &Photo<'_>, recipe: &Recipe, options: Options) -> Result<Image, Error> {
    block_on(async {
        let held = gpu.render(photo, recipe, &Presets::new(), options).await?;
        gpu.download(&held).await
    })
}

/// The largest difference between `cpu` and `gpu` (pictures of one size), value by value, and where it is: the one gap
/// every GPU test takes.
///
/// # Panics
/// If their lengths differ or either holds a value that is not finite: a NaN is no colour, and a maximum would drop it.
pub(crate) fn largest_gap(cpu: &[f32], gpu: &[f32]) -> (f32, usize) {
    assert_eq!(cpu.len(), gpu.len(), "the pictures differ in length");
    assert_finite(cpu, "the CPU's picture");
    assert_finite(gpu, "the GPU's picture");
    cpu.iter()
        .zip(gpu)
        .map(|(one, other)| (one - other).abs())
        .enumerate()
        .fold(
            (0.0, 0),
            |(gap, at), (index, here)| if here > gap { (here, index) } else { (gap, at) },
        )
}

/// # Panics
/// If a value of `values` (`what`) is NaN or an infinity, naming the first.
pub(crate) fn assert_finite(values: &[f32], what: &str) {
    let first = values.iter().position(|value| !value.is_finite());
    assert!(
        first.is_none(),
        "{what} holds {} at value {first:?}",
        first.map_or(0.0, |at| values[at])
    );
}

#[test]
fn the_gap_of_two_pictures_is_their_largest_difference_and_a_non_finite_value_fails_it() {
    assert_eq!(largest_gap(&[0.0, 0.5, 1.0], &[0.0, 0.25, 0.875]), (0.25, 1));
    assert_eq!(largest_gap(&[0.5], &[0.5]), (0.0, 0));
    for bad in [f32::NAN, f32::INFINITY] {
        let caught = std::panic::catch_unwind(|| largest_gap(&[0.5, 0.5], &[0.5, bad]));
        assert!(caught.is_err(), "{bad} on the GPU passed");
        let caught = std::panic::catch_unwind(|| largest_gap(&[bad, 0.5], &[0.5, 0.5]));
        assert!(caught.is_err(), "{bad} on the CPU passed");
    }
}

#[test]
fn the_adapter_is_the_one_bt_gpu_names_and_holds_a_picture() {
    // `fresh` checks the name; a device holds at least WebGPU's default binding, 128 MiB, 8 Mi pixels
    if let Some(gpu) = gpu() {
        assert!(gpu.largest_picture() >= 1 << 23, "{:?}", *gpu);
    }
}
