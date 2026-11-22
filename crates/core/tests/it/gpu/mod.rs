//! The GPU backend (feature `gpu`), at small sizes. `BT_GPU` says what GPU the machine has: unset, the tests need one
//! and fail without it; `none`, each GPU test says it was skipped and why, and passes; any other value is part of the
//! adapter's name the tests must find (CI sets `llvmpipe`, Mesa's software Vulkan).

mod parity;
mod render;
mod transfer;

use std::sync::LazyLock;

use bullterrier_image::Gpu;

/// The context the tests share, or none where `BT_GPU=none` says the machine has no GPU.
static SHARED: LazyLock<Option<Gpu>> = LazyLock::new(fresh);

/// The shared context, or none (after saying so) where the machine has no GPU.
pub(crate) fn gpu() -> Option<Gpu> {
    SHARED.clone()
}

/// A context of its own, for a test that counts transfers; none where the machine has no GPU.
///
/// # Panics
/// If there is no adapter and `BT_GPU` does not say `none`, or the adapter is not the one `BT_GPU` names.
#[expect(clippy::print_stderr, reason = "a skipped test says why, as BT_GPU=none asks")]
#[expect(clippy::panic, reason = "a missing GPU fails the tests that need it, with its error")]
pub(crate) fn fresh() -> Option<Gpu> {
    let expected = std::env::var("BT_GPU").unwrap_or_default();
    if expected == "none" {
        eprintln!("skipped: BT_GPU=none says this machine has no GPU");
        return None;
    }
    let gpu =
        Gpu::request_blocking().unwrap_or_else(|error| panic!("{error} (set BT_GPU=none on a machine without a GPU)"));
    assert!(
        gpu.adapter().to_lowercase().contains(&expected.to_lowercase()),
        "the adapter is {}, not the {expected:?} BT_GPU names",
        gpu.adapter()
    );
    Some(gpu)
}

#[test]
fn the_adapter_is_the_one_bt_gpu_names_and_holds_a_picture() {
    // `fresh` checks the name; a device holds at least WebGPU's default binding, 128 MiB, 8 Mi pixels
    if let Some(gpu) = gpu() {
        assert!(gpu.largest_picture() >= 1 << 23, "{gpu:?}");
    }
}
