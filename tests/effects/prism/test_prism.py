"""Behaviour of the `prism` effect: spectral copies of the picture scaled about a centre, weighted back to RGB."""

import math

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "prism"


def grey(values: np.ndarray) -> np.ndarray:
    """A picture with `values` (height, width) in all three channels."""
    return np.repeat(values.astype(np.float32)[..., None], 3, axis=2)


def test_three_samples_are_the_red_green_and_blue_copies_scaled_a_tenth_apart_from_a_white_dot() -> None:
    """The axis on the left edge of 101 columns (x = 0, row 50): the farthest corner is hypot(50, 100) = 111.8 px away
    and the amount, 0.4 of it, puts the red and the blue copy 0.4 apart in scale: red read at 0.8 of the distance, blue
    at 1.2, green at 1. A white dot 60 px out shows red 75 px out (60 / 0.8), green 60 and blue 50 (60 / 1.2)."""
    dot = np.zeros((101, 101, 3), np.float32)
    dot[50, 60] = 1.0
    out = registry.apply(NAME, dot, amount=0.4 * math.hypot(50, 100), samples=3, x=0.0, y=0.5, interpolation="nearest")
    assert np.argwhere(out[..., 0] > 0).tolist() == [[50, 75]]
    assert np.argwhere(out[..., 1] > 0).tolist() == [[50, 60]]
    assert np.argwhere(out[..., 2] > 0).tolist() == [[50, 50]]


def test_five_samples_weight_the_copies_by_the_spectrum_red_yellow_green_cyan_blue() -> None:
    """On 101 x 101 px the farthest corner is 70.71 px from the centre (50, 50); the amount 14.14 px is 0.2 of that, so
    the five copies are read at 0.9, 0.95, 1, 1.05 and 1.1 of each distance, and on a ramp column / 100 the pixel 40 px
    right of the centre shows 0.86, 0.88, 0.90, 0.92 and 0.94 of them. The five copies are red, yellow, green, cyan and
    blue: red is made of the first two (red and yellow), green of the middle three (yellow, green, cyan) and blue of the
    last two (cyan, blue), each averaged, so red is (0.86 + 0.88) / 2, green (0.88 + 0.90 + 0.92) / 3 and blue (0.92 +
    0.94) / 2."""
    ramp = grey(np.tile(np.arange(101) / 100.0, (101, 1)))
    out = registry.apply(NAME, ramp, amount=0.2 * 50 * math.sqrt(2), samples=5, interpolation="bilinear")
    assert out[50, 90] == pytest.approx([0.87, 0.90, 0.93], abs=1e-5)


@pytest.mark.parametrize("samples", [3, 4, 7, 32])
def test_a_flat_colour_stays_as_it_is_whatever_the_number_of_copies(samples: int) -> None:
    flat = np.full((21, 31, 3), (0.2, 0.5, 0.9), np.float32)
    out = registry.apply(NAME, flat, amount=30, samples=samples)
    np.testing.assert_allclose(out, flat, atol=1e-6)


@pytest.mark.parametrize(("x", "y"), [(0.5, 0.5), (0.2, 0.7)], ids=["middle", "off centre"])
def test_the_pixel_at_the_centre_is_never_changed(x: float, y: float) -> None:
    """Every copy reads the centre pixel from itself. On 51 x 51 px a centre at 0.2 / 0.7 is the pixel (35, 10)."""
    noise = np.random.default_rng(7).random((51, 51, 3), dtype=np.float32)
    row, col = round(y * 50), round(x * 50)
    out = registry.apply(NAME, noise, amount=40, x=x, y=y, samples=9)
    np.testing.assert_allclose(out[row, col], noise[row, col], atol=1e-6)
    assert np.abs(out - noise).max() > 0.05  # and elsewhere it is not


def test_a_shift_beyond_the_farthest_corner_is_limited_to_it() -> None:
    """The reddest copy cannot be pushed through the centre: at a shift of the whole corner distance (amount twice it)
    it reads every pixel at the centre, and a larger amount shows the same."""
    noise = np.random.default_rng(3).random((31, 41, 3), dtype=np.float32)
    corner = math.hypot(15, 20)
    reach = registry.apply(NAME, noise, amount=2 * corner, samples=3, interpolation="nearest")
    beyond = registry.apply(NAME, noise, amount=2 * corner * 1.9, samples=3, interpolation="nearest")
    np.testing.assert_array_equal(beyond, reach)
    np.testing.assert_allclose(reach[..., 0], noise[15, 20, 0], atol=1e-6)


@pytest.mark.parametrize(
    ("edge", "expected_blue"),
    [("nearest", 1.0), ("mirror", 0.5), ("reflect", 0.55), ("wrap", 0.45)],
    ids=["nearest", "mirror", "reflect", "wrap"],
)
def test_edge_sets_what_the_blue_copy_sees_beyond_the_picture(edge: str, expected_blue: float) -> None:
    """A picture of 21 columns with the value column / 20, the centre on its top left pixel: the farthest corner is
    28.28 px away, so the amount 28.28 puts the blue copy at 1.5 of each distance and the red at 0.5. The pixel (0, 20)
    reads blue at column 30, 10 beyond the last: nearest repeats the last column, 1.0; mirror reads column 10 (about
    column 20), 0.5; reflect column 11, 0.55; wrap column 9 (21 columns repeat), 0.45."""
    ramp = grey(np.tile(np.arange(21) / 20.0, (21, 1)))
    out = registry.apply(
        NAME, ramp, amount=math.hypot(20, 20), samples=3, x=0.0, y=0.0, edge=edge, interpolation="nearest"
    )
    assert out[0, 20, 2] == pytest.approx(expected_blue, abs=1e-6)
    assert out[0, 20, 0] == pytest.approx(0.5, abs=1e-6)
