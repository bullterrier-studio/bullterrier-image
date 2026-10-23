"""Behaviour of the `vignette` effect."""

import itertools

import numpy as np
import pytest

from photo_fx.core import registry

NAME = "vignette"


def grey(height: int, width: int, value: float = 0.5) -> np.ndarray:
    return np.full((height, width, 3), value, np.float32)


def brute_force(rgb: np.ndarray, amount: float, radius: float, feather: float, roundness: float) -> np.ndarray:
    """The vignette pixel by pixel, from its definition, for a black vignette.

    A pixel centre sits at ((2x + 1 - w) / w, (2y + 1 - h) / h): -1..1 across the picture. Its distance is the
    superellipse norm of that point, 1 at the corners and 0 in the centre; roundness 1 is an ellipse, smaller
    roundness squarer. Nothing happens inside `radius`; the shade then rises over `feather` as a smoothstep.
    """
    height, width = rgb.shape[:2]
    out = np.empty_like(rgb)
    for y in range(height):
        for x in range(width):
            nx, ny = abs(2 * x + 1 - width) / width, abs(2 * y + 1 - height) / height
            if roundness == 0:
                distance = max(nx, ny)
            else:
                p = 2 / roundness
                distance = ((nx**p + ny**p) / 2) ** (1 / p)
            t = min(max((distance - radius) / feather, 0.0), 1.0)
            shade = t * t * (3 - 2 * t)
            out[y, x] = rgb[y, x] * (1 - amount * shade)
    return out


def test_amount_zero_leaves_the_picture_unchanged(photo: np.ndarray) -> None:
    np.testing.assert_array_equal(registry.apply(NAME, photo, amount=0.0), photo)


def test_the_centre_is_untouched_and_the_corners_are_the_darkest() -> None:
    out = registry.apply(NAME, grey(41, 61), amount=1.0, radius=0.3, feather=0.4)
    assert out[20, 30, 0] == pytest.approx(0.5)
    assert out[20, 30, 0] == out[20, 30].max()
    corner = out[0, 0, 0]
    assert corner == out[..., 0].min()
    assert corner < 0.05  # amount 1 takes the corner almost to black


def test_the_vignette_is_symmetric_about_the_centre() -> None:
    out = registry.apply(NAME, grey(21, 33), amount=0.7, radius=0.2, feather=0.5)
    np.testing.assert_allclose(out, out[::-1], atol=1e-6)
    np.testing.assert_allclose(out, out[:, ::-1], atol=1e-6)


@pytest.mark.parametrize("height_width", [(30, 30), (20, 50), (50, 20)], ids=["square", "wide", "tall"])
def test_a_larger_radius_never_darkens_any_pixel(height_width: tuple[int, int]) -> None:
    picture = grey(*height_width)
    outs = [registry.apply(NAME, picture, amount=0.8, radius=r, feather=0.3) for r in (0.0, 0.25, 0.5, 0.75, 1.0)]
    for tighter, wider in itertools.pairwise(outs):
        assert (wider >= tighter - 1e-6).all()
    assert outs[-1].min() >= 0.5 - 1e-6  # at radius 1 even the corner is outside the vignette


@pytest.mark.parametrize(
    ("size", "settings"),
    [
        pytest.param(
            (9, 9), {"amount": 1.0, "radius": 0.0, "feather": 1.0, "roundness": 1.0}, id="circle, wide feather"
        ),
        pytest.param((8, 12), {"amount": 0.6, "radius": 0.3, "feather": 0.5, "roundness": 1.0}, id="wide ellipse"),
        pytest.param((12, 7), {"amount": 0.9, "radius": 0.4, "feather": 0.2, "roundness": 0.5}, id="tall, squarer"),
        pytest.param((10, 10), {"amount": 0.5, "radius": 0.1, "feather": 0.6, "roundness": 0.0}, id="rectangle"),
        pytest.param((1, 9), {"amount": 0.8, "radius": 0.2, "feather": 0.4, "roundness": 0.75}, id="a single row"),
    ],
)
def test_the_vignette_matches_a_pixel_by_pixel_computation(photo: np.ndarray, size, settings) -> None:
    picture = np.ascontiguousarray(photo[: size[0], : size[1]])
    expected = brute_force(picture, **settings)
    np.testing.assert_allclose(registry.apply(NAME, picture, **settings), expected, atol=2e-6)


def test_a_known_pixel_is_shaded_as_worked_out_by_hand() -> None:
    """On an 11 x 11 grey 0.5, pixel (row 5, column 10) is 10/11 = 0.909 of the way to the edge on one axis and in
    the middle on the other: ellipse distance 0.909 / sqrt(2) = 0.643; with radius 0.4 and feather 0.4 the shade is
    t = 0.607, 3t^2 - 2t^3 = 0.6581, so 0.5 * (1 - 0.8 * 0.6581) = 0.2367."""
    out = registry.apply(NAME, grey(11, 11), amount=0.8, radius=0.4, feather=0.4, roundness=1.0)
    assert out[5, 10, 0] == pytest.approx(0.2367, abs=2e-4)


def test_zero_roundness_darkens_the_edge_middles_as_much_as_the_corners_and_full_roundness_does_not() -> None:
    picture = grey(31, 31)
    square = registry.apply(NAME, picture, amount=1.0, radius=0.8, feather=0.15, roundness=0.0)
    round_ = registry.apply(NAME, picture, amount=1.0, radius=0.8, feather=0.15, roundness=1.0)
    assert square[15, 30, 0] == pytest.approx(square[0, 0, 0], abs=0.05)  # an edge middle is as dark as a corner
    assert round_[15, 30, 0] > 0.4  # but on the ellipse (distance 0.71 < 0.8) it is still lit
    assert round_[0, 0, 0] < 0.1


def test_color_is_the_colour_the_edges_fade_to() -> None:
    out = registry.apply(NAME, grey(21, 21, 0.5), amount=1.0, radius=0.0, feather=0.3, color="#ff0000")
    assert out[0, 0] == pytest.approx([1.0, 0.0, 0.0], abs=1e-5)  # the corner is the colour itself
    white = registry.apply(NAME, grey(21, 21, 0.5), amount=1.0, radius=0.0, feather=0.3, color="#ffffff")
    assert (white >= 0.5 - 1e-6).all()  # a white vignette only lightens


def test_a_feather_of_zero_is_a_hard_edge() -> None:
    out = registry.apply(NAME, grey(41, 41), amount=1.0, radius=0.5, feather=0.0, roundness=1.0)
    assert set(np.unique(out)) == {0.0, 0.5}


def test_the_centre_stays_untouched_even_with_no_radius_and_no_feather() -> None:
    out = registry.apply(NAME, grey(41, 41), amount=1.0, radius=0.0, feather=0.0)
    assert out[20, 20, 0] == 0.5  # the one pixel at distance 0 is inside the vignette's start, not past it
    assert out[20, 21, 0] == 0.0
