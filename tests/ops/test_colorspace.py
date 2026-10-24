"""Tests of `ops/colorspace.py`: the sRGB transfer function, HSV, JFIF YCbCr and NTSC YIQ, checked against published
values and the standard library's `colorsys`."""

import colorsys

import numpy as np
import pytest

from bullterrier_image.ops.colorspace import (
    hsv_to_rgb,
    linear_to_srgb,
    rgb_to_hsv,
    rgb_to_ycbcr,
    rgb_to_yiq,
    srgb_to_linear,
    ycbcr_to_rgb,
    yiq_to_rgb,
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


# 8-bit (Y, Cb, Cr) of the primaries and grey, as every JPEG tool tabulates them (JFIF, ITU-T T.871): the chroma of a
# grey is 128, red's Cr and blue's Cb are the top of the range (255.5, written 255 in 8 bits).
YCBCR_8BIT = [
    pytest.param((0, 0, 0), (0, 128, 128), id="black"),
    pytest.param((255, 255, 255), (255, 128, 128), id="white"),
    pytest.param((128, 128, 128), (128, 128, 128), id="grey"),
    pytest.param((255, 0, 0), (76.245, 84.972, 255.5), id="red"),
    pytest.param((0, 255, 0), (149.685, 43.528, 21.235), id="green"),
    pytest.param((0, 0, 255), (29.07, 255.5, 107.266), id="blue"),
]


@pytest.mark.parametrize(("rgb", "ycbcr"), YCBCR_8BIT)
def test_rgb_to_ycbcr_matches_the_jfif_values(rgb: tuple[int, ...], ycbcr: tuple[float, ...]) -> None:
    out = rgb_to_ycbcr(np.array(rgb, np.float32) / 255)
    np.testing.assert_allclose(out * 255, ycbcr, atol=0.01)


@pytest.mark.parametrize(("rgb", "ycbcr"), YCBCR_8BIT)
def test_ycbcr_to_rgb_matches_the_jfif_values(rgb: tuple[int, ...], ycbcr: tuple[float, ...]) -> None:
    np.testing.assert_allclose(ycbcr_to_rgb(np.array(ycbcr, np.float32) / 255) * 255, rgb, atol=0.01)


def test_ycbcr_to_rgb_undoes_rgb_to_ycbcr_and_keeps_the_shape() -> None:
    image = np.random.default_rng(5).random((4, 5, 3), dtype=np.float32)
    back = ycbcr_to_rgb(rgb_to_ycbcr(image))
    assert back.shape == image.shape
    np.testing.assert_allclose(back, image, atol=1e-5)


# (Y, I, Q) of the primaries and white: the columns of the FCC matrix as published (47 CFR 73.682), Y = 0.299 R +
# 0.587 G + 0.114 B, I = 0.5959 R - 0.2746 G - 0.3213 B, Q = 0.2115 R - 0.5227 G + 0.3112 B; a grey has no chroma.
YIQ = [
    pytest.param((0, 0, 0), (0, 0, 0), id="black"),
    pytest.param((1, 1, 1), (1, 0, 0), id="white has no chroma: the I and Q rows sum to zero"),
    pytest.param((1, 0, 0), (0.299, 0.5959, 0.2115), id="red"),
    pytest.param((0, 1, 0), (0.587, -0.2746, -0.5227), id="green"),
    pytest.param((0, 0, 1), (0.114, -0.3213, 0.3112), id="blue"),
]


@pytest.mark.parametrize(("rgb", "yiq"), YIQ)
def test_rgb_to_yiq_matches_the_fcc_coefficients(rgb: tuple[float, ...], yiq: tuple[float, ...]) -> None:
    np.testing.assert_allclose(rgb_to_yiq(np.array(rgb, np.float32)), yiq, atol=1e-6)


@pytest.mark.parametrize(("rgb", "yiq"), YIQ)
def test_yiq_to_rgb_matches_the_fcc_coefficients(rgb: tuple[float, ...], yiq: tuple[float, ...]) -> None:
    np.testing.assert_allclose(yiq_to_rgb(np.array(yiq, np.float32)), rgb, atol=1e-5)


def test_yiq_agrees_with_the_standard_librarys_rounder_ntsc_matrix_and_is_inverted_by_yiq_to_rgb() -> None:
    """`colorsys` uses the older three-digit coefficients (0.30, 0.59, 0.11, ...): close, not equal."""
    image = np.random.default_rng(6).random((5, 4, 3), dtype=np.float32)
    ours = rgb_to_yiq(image)
    for row in range(5):
        for column in range(4):
            np.testing.assert_allclose(ours[row, column], colorsys.rgb_to_yiq(*image[row, column]), atol=0.015)
    np.testing.assert_allclose(yiq_to_rgb(ours), image, atol=1e-5)
