"""Tests of `ops/colorspace.py`: the sRGB transfer function, checked against published values."""

import numpy as np
import pytest

from photo_fx.ops.colorspace import linear_to_srgb, srgb_to_linear

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


@pytest.mark.parametrize(("encoded", "linear"), SRGB_PAIRS)
def test_encoding_linear_light_matches_the_published_transfer_function(encoded: float, linear: float) -> None:
    np.testing.assert_allclose(linear_to_srgb(np.full(3, linear)), [encoded] * 3, atol=1e-6)


def test_encoding_undoes_decoding_over_the_whole_range_and_just_outside_it() -> None:
    values = np.linspace(-0.25, 1.25, 301, dtype=np.float32)
    triplets = np.stack([values, values[::-1], np.roll(values, 7)], axis=-1)
    np.testing.assert_allclose(linear_to_srgb(srgb_to_linear(triplets)), triplets, atol=1e-5)


def test_values_below_zero_are_mirrored_instead_of_becoming_nan() -> None:
    decoded = srgb_to_linear(np.array([-0.5, 0.5, 0.0]))
    np.testing.assert_allclose(decoded, [-0.21404114, 0.21404114, 0.0], atol=1e-6)


def test_any_leading_shape_is_kept_and_the_input_is_not_modified() -> None:
    image = np.random.default_rng(3).random((4, 5, 3), dtype=np.float32)
    before = image.copy()
    assert srgb_to_linear(image).shape == (4, 5, 3)
    assert linear_to_srgb(image[0]).shape == (5, 3)
    assert srgb_to_linear(image[0, 0]).shape == (3,)
    np.testing.assert_array_equal(image, before)


@pytest.mark.parametrize("shape", [(4,), (2, 2), (3, 4), ()], ids=["four", "two by two", "three by four", "scalar"])
def test_an_array_whose_last_axis_is_not_rgb_is_refused(shape: tuple[int, ...]) -> None:
    for convert in (srgb_to_linear, linear_to_srgb):
        with pytest.raises(ValueError, match="last axis"):
            convert(np.zeros(shape))
