"""`Gpu` and `render(..., gpu=)`: what Python code can break on the way to the core's GPU backend (the kernels' parity
is the core's own test). `BT_GPU=none` says the machine has no GPU: the tests that need one say so and skip; without
it a missing GPU fails them (docs/development.md)."""

import os

import numpy as np
import pytest

from bullterrier_image import Gpu, GpuError, Recipe, Step, fx, render, render_all

NO_GPU = os.environ.get("BT_GPU") == "none"
needs_gpu = pytest.mark.skipif(NO_GPU, reason="BT_GPU=none: this machine has no GPU")


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
    assert gpu.transfers == {"uploads": 2, "downloads": 2, "bytes": 2 * 30 * 40 * (12 + 16)}


@needs_gpu
def test_the_gpu_names_its_adapter(gpu):
    assert gpu.adapter
    assert gpu.adapter in repr(gpu)


@needs_gpu
def test_a_gpu_render_refuses_a_band_as_a_gpu_error_naming_it(gpu):
    with pytest.raises(GpuError, match="band="):
        render(photo(), Recipe([Step("grade")]), band=8, gpu=gpu)


def test_the_effects_say_which_run_on_the_gpu():
    assert fx.grade.gpu
    assert fx.levels.gpu
    assert not fx.vignette.gpu
