"""Tests of `ops/blur.py`: the Gaussian blur the effects share, against scipy's direct convolution as the oracle."""

import time

import numpy as np
import pytest
from scipy import ndimage

from photo_fx.ops.blur import Mode, gaussian_blur

MODES: list[Mode] = ["reflect", "mirror", "nearest", "wrap", "constant"]
TOLERANCE = 1e-5  # the bound `gaussian_blur` documents for pictures in 0..1
CONSTANT = 0.3  # beyond-the-edge value for mode 'constant': not 0, so a dropped value shows
# A single pixel, a single row or column, odd rectangles, and one larger than the biggest kernel radius below but one.
SIZES = [(1, 1), (1, 7), (7, 1), (13, 29), (61, 37), (150, 220)]
# Kernel radii (truncate 4) of 6, 28, 32, 80 and 360 px: small and large kernels, longer than most of the pictures.
SIGMAS = [1.5, 7.0, 8.0, 20.0, 90.0]


def picture(shape: tuple[int, ...], seed: int = 0) -> np.ndarray:
    """Gradients plus noise in 0..1, float32: edges that differ from each other, so every edge mode matters."""
    rng = np.random.default_rng(seed)
    rows, cols = np.mgrid[0 : shape[0], 0 : shape[1]].astype(np.float32)
    ramp = (rows / max(shape[0], 1) * 0.5 + cols / max(shape[1], 1) * 0.3).reshape(shape[:2] + (1,) * (len(shape) - 2))
    return (ramp + rng.random(shape, dtype=np.float32) * 0.2).astype(np.float32)


def scipy_blur(image: np.ndarray, sigma: float, mode: Mode, truncate: float = 4.0) -> np.ndarray:
    """The oracle: scipy's direct convolution over the two picture axes only, never across channels."""
    sigmas = (sigma, sigma) + (0.0,) * (image.ndim - 2)
    return ndimage.gaussian_filter(image, sigmas, mode=mode, cval=CONSTANT, truncate=truncate)


def blur(image: np.ndarray, sigma: float, mode: Mode, truncate: float = 4.0) -> np.ndarray:
    return gaussian_blur(image, sigma, truncate=truncate, mode=mode, constant_value=CONSTANT)


@pytest.mark.parametrize("sigma", SIGMAS)
@pytest.mark.parametrize("mode", MODES)
@pytest.mark.parametrize("size", SIZES, ids=[f"{h}x{w}" for h, w in SIZES])
def test_the_blur_matches_a_direct_convolution_within_the_documented_bound(size, mode, sigma):
    image = picture(size)
    got = blur(image, sigma, mode)
    assert got.shape == image.shape
    assert np.abs(got.astype(np.float64) - scipy_blur(image, sigma, mode)).max() <= TOLERANCE


@pytest.mark.parametrize("mode", MODES)
@pytest.mark.parametrize("sigma", [3.0, 40.0])
def test_each_channel_of_a_colour_picture_is_blurred_on_its_own(mode, sigma):
    image = picture((45, 70, 3), seed=1)
    image[..., 1] = 1.0 - image[..., 1]  # channels unlike each other: blurring across them would show
    got = blur(image, sigma, mode)
    assert np.abs(got.astype(np.float64) - scipy_blur(image, sigma, mode)).max() <= TOLERANCE


@pytest.mark.parametrize("truncate", [1.0, 2.5, 10.0])
def test_truncate_cuts_the_kernel_where_scipy_does(truncate):
    image = picture((90, 120, 3), seed=2)
    got = blur(image, 12.0, "reflect", truncate)
    assert np.abs(got.astype(np.float64) - scipy_blur(image, 12.0, "reflect", truncate)).max() <= TOLERANCE


@pytest.mark.parametrize(("sigma", "truncate"), [(0.0, 4.0), (1.5, 4.0), (7.0, 4.0), (2.0, 10.0), (30.0, 1.0)])
@pytest.mark.parametrize("mode", MODES)
def test_a_kernel_radius_below_32_px_gives_exactly_scipys_result(sigma, truncate, mode):
    image = picture((61, 83, 3), seed=3)
    np.testing.assert_array_equal(blur(image, sigma, mode, truncate), scipy_blur(image, sigma, mode, truncate))


@pytest.mark.parametrize("sigma", [2.0, 25.0, 120.0])
@pytest.mark.parametrize("mode", MODES)
def test_the_blur_never_leaves_the_range_of_the_picture_and_what_lies_beyond_it(mode, sigma):
    image = picture((70, 90), seed=4) * 0.5 + 0.4  # 0.4..0.9: the constant 0.3 lies below it
    got = blur(image, sigma, mode)
    low = min(float(image.min()), CONSTANT) if mode == "constant" else float(image.min())
    assert got.min() >= low
    assert got.max() <= image.max()


@pytest.mark.parametrize("sigma", [2.0, 50.0])
def test_the_blur_keeps_float32_and_leaves_its_input_alone(sigma):
    image = picture((40, 60, 3), seed=5)
    before = image.copy()
    got = gaussian_blur(image, sigma, truncate=4.0, mode="reflect")
    assert got.dtype == np.float32
    np.testing.assert_array_equal(image, before)


def test_mode_constant_needs_the_value_beyond_the_edge():
    with pytest.raises(ValueError, match="constant_value"):
        gaussian_blur(picture((8, 8)), 40.0, truncate=4.0, mode="constant")


def cpu_seconds(image: np.ndarray, sigma: float) -> float:
    """The least process CPU time of three blurs: the least is the one least disturbed by the rest of the machine."""
    times = []
    for _ in range(3):
        start = time.process_time()
        gaussian_blur(image, sigma, truncate=4.0, mode="reflect")
        times.append(time.process_time() - start)
    return min(times)


def test_a_large_blur_costs_a_few_times_a_small_one_not_a_hundred():
    """A direct convolution costs in proportion to the kernel: a 1201-tap one about 70 times a 17-tap one on the same
    picture. The blur promises a cost that hardly grows with sigma; the ratio, unlike a time, is the same on any
    machine."""
    image = picture((1000, 1000), seed=6)
    small, large = cpu_seconds(image, 2.0), cpu_seconds(image, 150.0)
    assert large < 15 * small
