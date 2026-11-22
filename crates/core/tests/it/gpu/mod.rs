//! The GPU backend (feature `gpu`), at small sizes. `BT_GPU` says what GPU the machine has: unset, the tests need one
//! and fail without it; `none`, each GPU test says it was skipped and why, and passes; any other value is part of the
//! adapter's name the tests must find (CI sets `llvmpipe`, Mesa's software Vulkan).

mod parity;
mod render;
mod transfer;

use std::sync::LazyLock;

use bullterrier_image::{Error, Gpu, Image, Options, Photo, Presets, Recipe};
use pollster::block_on;

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
        assert!(gpu.largest_picture() >= 1 << 23, "{gpu:?}");
    }
}
