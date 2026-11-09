"""Tests of `ops/colorspace.py`: the sRGB transfer function and Oklab, checked against published values."""

import numpy as np
import pytest

from bullterrier_image.ops.colorspace import (
    rgb_to_oklab,
    srgb_to_linear,
)

# (encoded, linear) pairs from the IEC 61966-2-1 transfer function, worked out by hand:
#   linear segment: v / 12.92 up to 0.04045; above it ((v + 0.055) / 1.055) ** 2.4.
SRGB_PAIRS = [
    pytest.param(0.0, 0.0, id="black"),
    pytest.param(0.04045, 0.04045 / 12.92, id="the top of the linear segment, 0.0031308"),
    pytest.param(0.02, 0.02 / 12.92, id="inside the linear segment"),
    pytest.param(0.5, 0.21404114, id="mid encoded grey is about 21% linear light (published: 0.2140)"),
    pytest.param(0.2, 0.03310477, id="a dark tone"),
    pytest.param(1.0, 1.0, id="white"),
]


@pytest.mark.parametrize(("encoded", "linear"), SRGB_PAIRS)
def test_decoding_srgb_matches_the_published_transfer_function(encoded: float, linear: float) -> None:
    np.testing.assert_allclose(srgb_to_linear(np.full(3, encoded)), [linear] * 3, atol=1e-6)


def test_values_below_zero_are_mirrored_instead_of_becoming_nan() -> None:
    decoded = srgb_to_linear(np.array([-0.5, 0.5, 0.0]))
    np.testing.assert_allclose(decoded, [-0.21404114, 0.21404114, 0.0], atol=1e-6)


def test_any_leading_shape_is_kept_and_the_input_is_not_modified() -> None:
    image = np.random.default_rng(3).random((4, 5, 3), dtype=np.float32)
    before = image.copy()
    assert srgb_to_linear(image).shape == (4, 5, 3)
    assert srgb_to_linear(image[0]).shape == (5, 3)
    assert srgb_to_linear(image[0, 0]).shape == (3,)
    np.testing.assert_array_equal(image, before)


# Oklab of the sRGB primaries and the greys at its ends, as Ottosson publishes them (three decimals).
OKLAB_PAIRS = [
    pytest.param((0.0, 0.0, 0.0), (0.0, 0.0, 0.0), id="black"),
    pytest.param((1.0, 1.0, 1.0), (1.0, 0.0, 0.0), id="white"),
    pytest.param((1.0, 0.0, 0.0), (0.628, 0.225, 0.126), id="red"),
    pytest.param((0.0, 1.0, 0.0), (0.867, -0.234, 0.180), id="green"),
    pytest.param((0.0, 0.0, 1.0), (0.452, -0.032, -0.312), id="blue"),
]


@pytest.mark.parametrize(("rgb", "lab"), OKLAB_PAIRS)
def test_rgb_to_oklab_matches_the_published_values(rgb: tuple[float, ...], lab: tuple[float, ...]) -> None:
    np.testing.assert_allclose(rgb_to_oklab(np.array(rgb)), lab, atol=1e-3)


def test_a_grey_has_no_colour_in_oklab_and_lightness_rises_with_it() -> None:
    greys = np.repeat(np.linspace(0, 1, 11, dtype=np.float32)[:, None], 3, axis=1)
    lab = rgb_to_oklab(greys)
    np.testing.assert_allclose(lab[:, 1:], 0.0, atol=1e-4)
    assert (np.diff(lab[:, 0]) > 0).all()


def test_oklab_keeps_any_leading_shape_and_does_not_modify_its_input() -> None:
    rgb = np.random.default_rng(2).random((3, 4, 5, 3), dtype=np.float32)
    before = rgb.copy()
    assert rgb_to_oklab(rgb).shape == rgb.shape
    assert rgb_to_oklab(rgb).dtype == np.float32
    np.testing.assert_array_equal(rgb, before)
