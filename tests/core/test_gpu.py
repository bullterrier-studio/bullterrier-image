"""`Gpu` and `render(..., gpu=)`: what Python code can break on the way to the core's GPU backend (the kernels' parity
is the core's own test). `BT_GPU=none` says the machine has no GPU: the tests that need one say so and skip; without
it a missing GPU fails them (docs/development.md)."""

import os
import sys

import numpy as np
import pytest

from bullterrier_image import Gpu, GpuError, Recipe, RecipeError, Step, fx, render, render_all
from bullterrier_image.core import native

NO_GPU = os.environ.get("BT_GPU") == "none"
needs_gpu = pytest.mark.skipif(NO_GPU, reason="BT_GPU=none: this machine has no GPU")


@pytest.mark.skipif(
    sys.platform == "darwin" or os.environ.get("BT_GPU_EXPERIMENTAL") == "1",
    reason="the GPU backend runs on macOS, and elsewhere only with BT_GPU_EXPERIMENTAL=1",
)
def test_a_gpu_is_refused_by_name_on_a_platform_the_backend_does_not_support():
    # not behind BT_GPU=none: the refusal needs no GPU, only the wheel built with the backend
    refusal = r"supports macOS and the browser; this is (linux|windows).*BT_GPU_EXPERIMENTAL=1"
    with pytest.raises(GpuError, match=refusal):
        Gpu()


@pytest.fixture(scope="module")
def gpu() -> Gpu:
    return Gpu()


def photo() -> np.ndarray:
    return np.random.default_rng(7).random((30, 40, 3), dtype=np.float32)


@needs_gpu
def test_a_render_on_the_gpu_is_the_cpu_render_within_the_kernels_bound(gpu):
    recipe = Recipe([fx.grade(contrast=1.4, saturation=1.2), fx.levels(black=0.1)])
    on_cpu = render(photo(), recipe)
    on_gpu = render(photo(), recipe, gpu=gpu)
    assert on_gpu.dtype == np.float32
    assert on_gpu.shape == on_cpu.shape
    assert float(np.abs(on_gpu - on_cpu).max()) <= 1e-4


@needs_gpu
def test_an_all_kernel_recipe_crosses_once_each_way_and_render_all_once_a_recipe():
    gpu = Gpu()
    render(photo(), Recipe([fx.grade(contrast=1.4)]), gpu=gpu)
    assert (gpu.transfers["uploads"], gpu.transfers["downloads"]) == (1, 1)
    gpu.reset_transfers()
    render_all(photo(), [Recipe([fx.levels(gamma=1.5)]), Recipe([fx.curves(midtones=0.6)])], gpu=gpu)
    # the curves' table (4 curves of 4 stretches of 4 coefficients: 256 bytes) is counted beside the pictures
    assert gpu.transfers == {"uploads": 2, "tables": 1, "downloads": 2, "bytes": 2 * 30 * 40 * (12 + 16) + 256}


@needs_gpu
def test_the_gpu_names_its_adapter_and_the_largest_picture_it_holds(gpu):
    assert gpu.adapter
    assert gpu.adapter in repr(gpu)
    # any device holds WebGPU's default binding, 128 MiB: 8 Mi pixels of 16 bytes
    assert gpu.largest_picture >= 1 << 23


@needs_gpu
def test_a_gpu_render_refuses_a_band_as_a_recipe_error_naming_it_never_a_gpu_error(gpu):
    with pytest.raises(RecipeError, match="band=") as refused:
        render(photo(), Recipe([Step("grade")]), band=8, gpu=gpu)
    assert not isinstance(refused.value, GpuError)


# Written out, not read from the core: the table of "what runs where" in docs/design-decisions.md ("GPU backend").
# A kernel added or dropped changes this list on purpose.
KERNELS = frozenset(
    [
        "aurora",
        "channel_mixer",
        "color_balance",
        "crosshatch",
        "curves",
        "dead_pixels",
        "dither",
        "engraving",
        "fog",
        "fractal",
        "grade",
        "gradient",
        "gradient_map",
        "grain",
        "halftone",
        "interlace",
        "lens_flare",
        "levels",
        "light_leak",
        "lut",
        "noise_field",
        "ntsc",
        "paper_texture",
        "pattern",
        "pixel_grid",
        "scanlines",
        "selective_color",
        "slice_shift",
        "torn_edge",
        "vignette",
    ]
)


def test_every_effect_says_it_runs_on_the_gpu_exactly_when_the_core_has_a_kernel_for_it():
    on_gpu = {name for name in fx.__all__ if getattr(fx, name).gpu}
    with_kernels = {name for name in fx.__all__ if native.effect(name)["gpu"]}
    assert on_gpu == with_kernels
    # known-good rows report true, known-bad ones (an effect with no kernel) false, from a list the core does not write
    assert on_gpu == KERNELS
    assert not on_gpu & {"blend", "vhs", "bloom"}
