"""Behaviour of the `emboss` effect."""

import numpy as np
import pytest

from bullterrier_image.core import registry

NAME = "emboss"
SIZE = 40
SLOPE = 0.005  # brightness gained per pixel along a test ramp


def ramp(direction: float) -> np.ndarray:
    """A grey picture whose brightness rises by SLOPE per px towards `direction` (degrees clockwise from up)."""
    y, x = np.mgrid[0:SIZE, 0:SIZE].astype(np.float64)
    grey = 0.5 + SLOPE * (x * np.sin(np.radians(direction)) - y * np.cos(np.radians(direction)))
    return np.repeat(grey[..., None], 3, axis=-1).astype(np.float32)


def relief(slope: float) -> float:
    """The shade of a surface rising by `slope` per px away from the light, worked out by hand: its unit normal leans
    towards the light by slope / sqrt(1 + slope^2); mid grey 0.5 is flat, 1 faces the light edge-on and 0 faces away."""
    return 0.5 + 0.5 * slope / np.sqrt(1 + slope**2)


def test_a_flat_picture_is_mid_grey_whatever_the_light() -> None:
    flat = np.full((SIZE, SIZE, 3), (0.9, 0.2, 0.4), np.float32)
    for edge in ("reflect", "nearest", "mirror", "wrap"):
        np.testing.assert_allclose(registry.apply(NAME, flat, angle=77, height=5, edge=edge), 0.5, atol=1e-6)


@pytest.mark.parametrize(
    ("light", "direction", "height", "sign"),
    [
        pytest.param(270, 90, 1.0, +1, id="light from the left, brightness rising to the right: surface faces it"),
        pytest.param(90, 90, 1.0, -1, id="light from the right, brightness rising to the right: faces away"),
        pytest.param(0, 0, 4.0, -1, id="light from above, brightness rising upwards: faces away"),
        pytest.param(180, 0, 4.0, +1, id="light from below, brightness rising upwards: faces it"),
        pytest.param(0, 90, 8.0, 0, id="light from above, ramp rising sideways: flat across the light"),
        pytest.param(315, 45, 8.0, 0, id="light from the upper left, ramp rising to the upper right: across"),
        pytest.param(315, 135, 8.0, +1, id="diagonal ramp, diagonal light, worked by cos(light - direction)"),
        pytest.param(0, 0, 50.0, -1, id="a steep vertical ramp: the slope is normalised, not unbounded"),
        pytest.param(315, 135, 50.0, +1, id="a steep diagonal ramp: both gradient components are normalised"),
    ],
)
def test_on_a_ramp_the_shade_follows_the_cosine_of_the_angle_between_light_and_ramp(
    light: float, direction: float, height: float, sign: int
) -> None:
    out = registry.apply(NAME, ramp(direction), angle=light, height=height)
    cosine = np.cos(np.radians(light - direction))
    expected = relief(-height * SLOPE * cosine)
    inside = out[5:-5, 5:-5]
    np.testing.assert_allclose(inside, expected, atol=2e-4)
    assert np.sign(round(float(inside.mean()) - 0.5, 3)) == sign  # lighter, darker or flat, as the id says


def test_the_opposite_light_gives_the_inverse_picture(photo: np.ndarray) -> None:
    out = registry.apply(NAME, photo, angle=30, height=3)
    np.testing.assert_allclose(registry.apply(NAME, photo, angle=210, height=3), 1 - out, atol=1e-6)


def test_height_0_is_flat_and_a_steeper_height_never_leaves_the_range(photo: np.ndarray) -> None:
    np.testing.assert_allclose(registry.apply(NAME, photo, height=0), 0.5, atol=1e-6)
    steep = registry.apply(NAME, photo, height=50)
    assert steep.min() < 0.1
    assert steep.max() > 0.9


@pytest.mark.parametrize(
    ("edge", "first_column_slope"),
    [
        pytest.param("reflect", SLOPE / 2, id="reflect repeats the edge pixel: half the slope"),
        pytest.param("nearest", SLOPE / 2, id="nearest repeats the edge pixel: half the slope"),
        pytest.param("mirror", 0.0, id="mirror reflects about the edge pixel: no slope"),
        pytest.param("wrap", -SLOPE * (SIZE - 2) / 2, id="wrap sees the far side: a steep drop"),
    ],
)
def test_edge_decides_the_slope_in_the_first_column_and_nowhere_else(edge: str, first_column_slope: float) -> None:
    out = registry.apply(NAME, ramp(90), angle=270, height=1, edge=edge)
    np.testing.assert_allclose(out[:, 0], relief(first_column_slope), atol=2e-4)
    np.testing.assert_allclose(out[:, 1:-1], relief(SLOPE), atol=2e-4)


def test_saturation_brings_back_the_pictures_colours(photo: np.ndarray) -> None:
    grey = registry.apply(NAME, photo, saturation=0)
    coloured = registry.apply(NAME, photo, saturation=1)
    half = registry.apply(NAME, photo, saturation=0.5)
    np.testing.assert_allclose(grey[..., 0], grey[..., 1])  # no colour at 0
    assert np.abs(coloured[..., 0] - coloured[..., 2]).mean() > 0.05  # colour at 1
    assert (coloured > photo + 0.01).any()  # the side facing the light is brighter than the picture
    assert (coloured < photo - 0.01).any()  # and the side facing away darker
    np.testing.assert_allclose(half, 0.5 * grey + 0.5 * coloured, atol=1e-6)


def test_saturation_1_leaves_a_flat_picture_as_it_is() -> None:
    flat = np.full((SIZE, SIZE, 3), (0.9, 0.2, 0.4), np.float32)
    np.testing.assert_allclose(registry.apply(NAME, flat, saturation=1), flat, atol=1e-6)


def test_the_relief_follows_brightness_not_any_one_channel() -> None:
    """Grey 0.5 and pure green of the same Rec. 709 luminance (0.7152 x 0.69911 = 0.5) make no relief."""
    same_brightness = np.full((SIZE, SIZE, 3), 0.5, np.float32)
    same_brightness[:, SIZE // 2 :] = (0.0, 0.5 / 0.7152, 0.0)
    np.testing.assert_allclose(registry.apply(NAME, same_brightness, height=50), 0.5, atol=1e-5)
