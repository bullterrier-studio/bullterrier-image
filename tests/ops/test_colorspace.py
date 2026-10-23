"""Tests of `ops/colorspace.py`: the sRGB transfer function and HSV, checked against published values and the standard
library's `colorsys`."""

import colorsys

import numpy as np
import pytest

from photo_fx.ops.colorspace import hsv_to_rgb, linear_to_srgb, rgb_to_hsv, srgb_to_linear

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


HSV_PAIRS = [
    pytest.param((1, 0, 0), (0, 1, 1), id="red is hue 0"),
    pytest.param((1, 1, 0), (1 / 6, 1, 1), id="yellow is a sixth of the way round"),
    pytest.param((0, 1, 0), (1 / 3, 1, 1), id="green is a third"),
    pytest.param((0, 1, 1), (0.5, 1, 1), id="cyan is half way"),
    pytest.param((0, 0, 1), (2 / 3, 1, 1), id="blue is two thirds"),
    pytest.param((1, 0, 1), (5 / 6, 1, 1), id="magenta is five sixths"),
    pytest.param((0.5, 0.25, 0.25), (0, 0.5, 0.5), id="a muted red"),
    pytest.param((0.5, 0.5, 0.5), (0, 0, 0.5), id="grey has hue 0 and no saturation"),
    pytest.param((0, 0, 0), (0, 0, 0), id="black has no saturation either"),
]


@pytest.mark.parametrize(("rgb", "hsv"), HSV_PAIRS)
def test_rgb_to_hsv_matches_the_textbook_values(rgb: tuple[float, ...], hsv: tuple[float, ...]) -> None:
    np.testing.assert_allclose(rgb_to_hsv(np.array(rgb)), hsv, atol=1e-6)


@pytest.mark.parametrize(("rgb", "hsv"), HSV_PAIRS)
def test_hsv_to_rgb_matches_the_textbook_values(rgb: tuple[float, ...], hsv: tuple[float, ...]) -> None:
    np.testing.assert_allclose(hsv_to_rgb(np.array(hsv)), rgb, atol=1e-6)


def test_hsv_agrees_with_the_standard_library_in_both_directions() -> None:
    colours = np.random.default_rng(5).random((200, 3), dtype=np.float32)
    expected_hsv = np.array([colorsys.rgb_to_hsv(*map(float, c)) for c in colours])
    np.testing.assert_allclose(rgb_to_hsv(colours), expected_hsv, atol=1e-5)
    np.testing.assert_allclose(hsv_to_rgb(expected_hsv), colours, atol=1e-5)


def test_hue_is_always_below_one_even_for_a_red_with_a_trace_of_blue() -> None:
    reds = np.array([[1.0, 0.0, 1e-9], [1.0, 0.0, 0.0]], dtype=np.float32)
    assert (rgb_to_hsv(reds)[:, 0] < 1).all()


def test_hue_wraps_around_the_colour_circle() -> None:
    np.testing.assert_allclose(hsv_to_rgb(np.array([1.25, 1, 1])), hsv_to_rgb(np.array([0.25, 1, 1])), atol=1e-6)
    np.testing.assert_allclose(hsv_to_rgb(np.array([-0.75, 1, 1])), hsv_to_rgb(np.array([0.25, 1, 1])), atol=1e-6)


def test_the_last_axis_of_hsv_conversions_must_be_three() -> None:
    for convert in (rgb_to_hsv, hsv_to_rgb):
        with pytest.raises(ValueError, match="last axis"):
            convert(np.zeros((2, 4)))
