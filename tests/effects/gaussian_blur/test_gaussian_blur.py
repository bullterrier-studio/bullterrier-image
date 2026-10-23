"""Behaviour of the `gaussian_blur` effect."""

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "gaussian_blur"


def kernel_1d(sigma: float, truncate: float) -> np.ndarray:
    """The Gaussian weights at integer offsets, from the definition exp(-d^2 / 2 sigma^2), cut off at truncate x sigma
    and summing to 1: the discrete kernel of a blur of standard deviation `sigma` px."""
    reach = int(truncate * sigma + 0.5)
    offsets = np.arange(-reach, reach + 1)
    weights = np.exp(-(offsets**2) / (2.0 * sigma**2))
    return weights / weights.sum()


def impulse(size: int) -> np.ndarray:
    """A black picture with one white pixel in the middle."""
    dark = np.zeros((size, size, 3), np.float32)
    dark[size // 2, size // 2] = 1.0
    return dark


@pytest.mark.parametrize("radius", [3.0, 40.0], ids=["a short kernel", "a kernel longer than 100 px"])
def test_an_impulse_becomes_the_gaussian_of_that_sigma(radius: float) -> None:
    """The response to one white pixel is the outer product of the 1-D Gaussian weights, worked out in the test."""
    size = 401
    centre = size // 2
    weights = kernel_1d(radius, truncate=4.0)
    reach = len(weights) // 2
    expected = np.zeros((size, size))
    expected[centre - reach : centre + reach + 1, centre - reach : centre + reach + 1] = np.outer(weights, weights)
    out = registry.apply(NAME, impulse(size), radius=radius, truncate=4.0, mode="constant")
    for channel in range(3):
        np.testing.assert_allclose(out[..., channel], expected, atol=2e-6)


@pytest.mark.parametrize("mode", ["reflect", "mirror", "nearest", "wrap", "constant"])
def test_a_constant_picture_stays_as_it_is_whatever_lies_beyond_its_edge(mode: str) -> None:
    grey = np.full((21, 34, 3), 0.4, np.float32)
    out = registry.apply(NAME, grey, radius=6.0, mode=mode, constant_value=0.4)
    np.testing.assert_allclose(out, 0.4, atol=1e-6)


def test_the_blur_keeps_the_mean_when_the_picture_wraps_around(photo: np.ndarray) -> None:
    """With 'wrap' nothing leaves or enters the picture, so the weights summing to 1 keep each channel's mean."""
    out = registry.apply(NAME, photo, radius=5.0, mode="wrap")
    np.testing.assert_allclose(out.mean(axis=(0, 1)), photo.mean(axis=(0, 1)), atol=1e-5)
    assert out.std() < photo.std()  # and it did blur


def test_radius_zero_leaves_the_picture_unchanged(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, radius=0.0), photo)


def test_each_colour_channel_is_blurred_on_its_own() -> None:
    red_dot = impulse(41)
    red_dot[..., 1:] = 0.0
    out = registry.apply(NAME, red_dot, radius=3.0)
    assert out[20, 24, 0] > 0
    assert out[..., 1:].max() == 0.0


def test_radius_sets_the_standard_deviation_of_the_spread() -> None:
    """The variance of the blurred impulse along a row is sigma^2 (a Gaussian well inside the picture)."""
    out = registry.apply(NAME, impulse(121), radius=5.0, truncate=4.0, mode="constant")
    row = out[60, :, 0].astype(np.float64)
    offsets = np.arange(121) - 60
    variance = float((row * offsets**2).sum() / row.sum())
    assert variance == pytest.approx(5.0**2, rel=0.01)


def test_truncate_sets_where_the_kernel_is_cut_off() -> None:
    at_six_px = (40, 46)  # 3 sigma out for radius 2
    assert registry.apply(NAME, impulse(81), radius=2.0, truncate=4.0)[at_six_px][0] > 0
    assert registry.apply(NAME, impulse(81), radius=2.0, truncate=2.0)[at_six_px][0] == 0


@pytest.mark.parametrize(
    ("mode", "constant_value", "beyond"),
    [
        ("constant", 0.0, 0.0),
        ("constant", 1.0, 1.0),
        ("constant", 0.8, 0.8),
        ("reflect", 0.0, 0.5),
        ("nearest", 1.0, 0.5),
    ],
    ids=["dark beyond", "light beyond", "grey beyond", "reflect repeats the grey", "nearest repeats the grey"],
)
def test_mode_and_constant_value_set_what_the_blur_sees_beyond_the_edge(
    mode: str, constant_value: float, beyond: float
) -> None:
    """On a 0.5 grey the corner pixel sees the picture only in one quadrant, whose share of the kernel is the square
    of the 1-D weights at offsets 0 and up; the rest of its neighbourhood is `beyond`."""
    quadrant = kernel_1d(4.0, truncate=4.0)[16:].sum() ** 2
    grey = np.full((41, 41, 3), 0.5, np.float32)
    out = registry.apply(NAME, grey, radius=4.0, mode=mode, constant_value=constant_value)
    np.testing.assert_allclose(out[0, 0], beyond + (0.5 - beyond) * quadrant, atol=1e-5)
